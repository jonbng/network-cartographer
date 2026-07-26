use std::collections::{BTreeSet, HashMap, HashSet};
use std::net::IpAddr;
use std::time::{Duration, Instant};

use crate::resolve::HostnameCache;

use super::{
    is_local_or_private, AppEntry, AttributionSource, AttributionStats, Connection,
    ConnectionObservation, DestKey, DestStats, Protocol, SocketKey, SocketTrafficCounters,
    TrafficRate, UnattributedReason,
};

#[derive(Debug)]
pub struct AppState {
    apps: HashMap<String, AppEntry>,
    unattributed: AppEntry,
    pub retain: Duration,
    pub include_udp: bool,
    pub missing_pid_count: usize,
    pub last_poll_at: Option<Instant>,
    pub poll_count: u64,
    /// Snapshot connection count from last poll (after filters).
    pub last_raw_connections: usize,
    pub attribution: AttributionStats,
    socket_traffic: HashMap<SocketKey, SocketTrafficCounters>,
}

impl AppState {
    pub fn new(retain: Duration, include_udp: bool) -> Self {
        Self {
            apps: HashMap::new(),
            unattributed: empty_entry(
                "Unattributed traffic",
                None,
                "__unattributed__",
                Instant::now(),
            ),
            retain,
            include_udp,
            missing_pid_count: 0,
            last_poll_at: None,
            poll_count: 0,
            last_raw_connections: 0,
            attribution: AttributionStats::default(),
            socket_traffic: HashMap::new(),
        }
    }

    pub fn reset(&mut self) {
        self.apps.clear();
        self.unattributed = empty_entry(
            "Unattributed traffic",
            None,
            "__unattributed__",
            Instant::now(),
        );
        self.missing_pid_count = 0;
        self.poll_count = 0;
        self.last_raw_connections = 0;
        self.last_poll_at = None;
        self.attribution = AttributionStats::default();
        self.socket_traffic.clear();
    }

    pub fn ingest(
        &mut self,
        connections: Vec<ConnectionObservation>,
        hostnames: &mut HostnameCache,
    ) {
        let now = Instant::now();
        let dt = self
            .last_poll_at
            .map(|t| now.duration_since(t).as_secs_f64().max(0.2))
            .unwrap_or(1.0);
        self.last_poll_at = Some(now);
        self.poll_count += 1;

        let mut missing = 0usize;
        let mut kept = 0usize;
        let mut current_counter_keys = HashSet::new();

        for app in self.apps.values_mut() {
            app.current_connections = 0;
            app.hits_per_sec = 0.0;
            app.traffic = None;
            app.pids.clear();
            app.processes.clear();
        }
        self.unattributed.current_connections = 0;
        self.unattributed.hits_per_sec = 0.0;
        self.unattributed.traffic = None;
        self.unattributed.pids.clear();
        self.unattributed.processes.clear();
        self.attribution = AttributionStats::default();

        for observation in connections {
            let active = observation.active;
            let conn = observation.connection;
            if matches!(conn.protocol, Protocol::Udp) && !self.include_udp {
                continue;
            }
            // Skip pure listeners with no remote peer
            if conn.remote.ip().is_unspecified() || conn.remote.port() == 0 {
                continue;
            }
            if is_local_or_private(conn.remote.ip()) {
                continue;
            }
            if active {
                kept += 1;
            }
            hostnames.request(conn.remote.ip());
            let matched_name = conn.destination_name.clone();
            let processes = conn.processes.clone();

            let socket_key = conn.socket_key();
            let traffic_delta = active
                .then_some(conn.traffic_counters)
                .flatten()
                .map(|current| {
                    current_counter_keys.insert(socket_key.clone());
                    let previous = self.socket_traffic.insert(socket_key, current);
                    previous
                        .map(|previous| SocketTrafficCounters {
                            rx_bytes: current.rx_bytes.saturating_sub(previous.rx_bytes),
                            tx_bytes: current.tx_bytes.saturating_sub(previous.tx_bytes),
                        })
                        .unwrap_or_default()
                });

            let entry = match conn.attribution {
                AttributionSource::Direct => {
                    if active {
                        self.attribution.direct += 1;
                    }
                    let key = app_key(&conn);
                    let display = connection_display_name(&conn, &key);
                    self.apps.entry(key.clone()).or_insert_with(|| {
                        empty_entry(&display, conn.process_path.clone(), &key, now)
                    })
                }
                AttributionSource::Recovered => {
                    if active {
                        self.attribution.recovered += 1;
                    }
                    let key = app_key(&conn);
                    let display = connection_display_name(&conn, &key);
                    self.apps.entry(key.clone()).or_insert_with(|| {
                        empty_entry(&display, conn.process_path.clone(), &key, now)
                    })
                }
                AttributionSource::Unattributed => {
                    if active {
                        missing += 1;
                        self.attribution.unattributed += 1;
                        if matches!(
                            conn.unattributed_reason,
                            Some(UnattributedReason::Ambiguous)
                        ) {
                            self.attribution.ambiguous += 1;
                        }
                        if matches!(
                            conn.unattributed_reason,
                            Some(UnattributedReason::OwnerGone)
                        ) {
                            self.attribution.owner_gone += 1;
                        }
                        if matches!(
                            conn.unattributed_reason,
                            Some(UnattributedReason::AccessLimited)
                        ) {
                            self.attribution.access_limited += 1;
                        }
                    }
                    &mut self.unattributed
                }
            };

            entry.last_seen = now;
            if !matches!(conn.attribution, AttributionSource::Unattributed)
                && !conn.process_name.is_empty()
                && conn.process_name != "unknown"
            {
                entry.name = conn.process_name.clone();
            }
            if active {
                entry.current_connections += 1;
            }
            if let Some(delta) = traffic_delta {
                let rate = entry.traffic.get_or_insert(TrafficRate {
                    sample_window_ms: (dt * 1000.0).round() as u64,
                    ..TrafficRate::default()
                });
                rate.rx_bytes_per_sec += delta.rx_bytes as f64 / dt;
                rate.tx_bytes_per_sec += delta.tx_bytes as f64 / dt;
            }
            if let Some(pid) = conn.pid {
                entry.pids.insert(pid);
            }
            if entry.path.is_none() {
                entry.path = conn.process_path.clone();
            }
            for process in &processes {
                entry.pids.insert(process.pid);
                entry.processes.insert(process.id.clone(), process.clone());
            }
            let dest_key = DestKey::from_remote(conn.remote, conn.protocol);
            let hostname = hostnames.get(conn.remote.ip());
            let dest = entry
                .destinations
                .entry(dest_key)
                .or_insert_with(|| DestStats {
                    remote: conn.remote,
                    hostname: hostname.clone(),
                    sni: None,
                    domain: matched_name.clone(),
                    protocol: conn.protocol,
                    process_ids: processes.iter().map(|process| process.id.clone()).collect(),
                    hit_count: 0,
                    active_observations: 0,
                    first_seen: now,
                    last_seen: now,
                });
            if conn.is_new {
                dest.hit_count += 1;
                entry.hits_per_sec += 1.0 / dt;
            }
            if active {
                dest.active_observations = dest.active_observations.saturating_add(1);
            }
            dest.last_seen = now;
            dest.protocol = conn.protocol;
            dest.process_ids
                .extend(processes.iter().map(|process| process.id.clone()));
            if let Some(name) = matched_name {
                let replace = dest.domain.as_ref().is_none_or(|current| {
                    current.is_expired(now)
                        || name.confidence > current.confidence
                        || name.source.priority() > current.source.priority()
                });
                if replace {
                    dest.sni = (name.source == super::DestinationNameSource::TlsSni)
                        .then(|| name.value.clone());
                    dest.domain = Some(name);
                }
            }
            if dest.hostname.is_none() {
                dest.hostname = hostname;
            } else if let Some(h) = hostname {
                dest.hostname = Some(h);
            }
        }

        self.missing_pid_count = missing;
        self.last_raw_connections = kept;
        self.socket_traffic
            .retain(|key, _| current_counter_keys.contains(key));

        // Process ownership describes the current snapshot. Keeping helper
        // identities forever made long-running applications accumulate stale
        // PIDs and left destination references pointing at dead processes.
        for app in self
            .apps
            .values_mut()
            .chain(std::iter::once(&mut self.unattributed))
        {
            let current_processes: HashSet<_> = app.processes.keys().cloned().collect();
            for destination in app.destinations.values_mut() {
                destination
                    .process_ids
                    .retain(|id| current_processes.contains(id));
            }
        }

        // Age out stale destinations / apps
        let retain = self.retain;
        self.apps.retain(|_, app| {
            app.destinations
                .retain(|_, d| now.duration_since(d.last_seen) <= retain);
            !app.destinations.is_empty() && now.duration_since(app.last_seen) <= retain
        });
        self.unattributed
            .destinations
            .retain(|_, d| now.duration_since(d.last_seen) <= retain);

        // Refresh hostnames.
        for app in self
            .apps
            .values_mut()
            .chain(std::iter::once(&mut self.unattributed))
        {
            for dest in app.destinations.values_mut() {
                if dest
                    .domain
                    .as_ref()
                    .is_some_and(|name| name.is_expired(now))
                {
                    dest.domain = None;
                    dest.sni = None;
                }
                if let Some(h) = hostnames.get(dest.remote.ip()) {
                    dest.hostname = Some(h);
                }
            }
        }
    }

    pub fn clear_destination_names(&mut self) {
        for app in self
            .apps
            .values_mut()
            .chain(std::iter::once(&mut self.unattributed))
        {
            for dest in app.destinations.values_mut() {
                dest.domain = None;
                dest.sni = None;
            }
        }
    }

    pub fn sorted_apps(&self) -> Vec<&AppEntry> {
        let mut list: Vec<_> = self.apps.values().collect();
        list.sort_by(|a, b| {
            b.connection_hits()
                .cmp(&a.connection_hits())
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        list
    }

    pub fn total_destinations(&self) -> usize {
        self.apps.values().map(|a| a.destinations.len()).sum()
    }

    pub fn app_count(&self) -> usize {
        self.apps.len()
    }

    pub fn unattributed(&self) -> &AppEntry {
        &self.unattributed
    }

    /// Attributed remote IPs with enough TCP evidence for automatic
    /// traceroute. UDP-only destinations remain available for explicit
    /// tracing without filling the background queue.
    pub fn auto_trace_ips(&self) -> Vec<IpAddr> {
        let mut evidence: HashMap<IpAddr, (u64, u64)> = HashMap::new();
        for app in self.apps.values() {
            for dest in app.destinations.values() {
                if matches!(dest.protocol, Protocol::Tcp) {
                    let totals = evidence.entry(dest.remote.ip()).or_default();
                    totals.0 = totals.0.saturating_add(dest.hit_count);
                    totals.1 = totals.1.saturating_add(dest.active_observations);
                }
            }
        }
        let mut ips: Vec<_> = evidence
            .into_iter()
            .filter_map(|(ip, (opens, live_observations))| {
                (opens >= 2 || live_observations >= 2).then_some(ip)
            })
            .collect();
        ips.sort();
        ips
    }

    /// Every retained remote IP. Explicit "Trace all" actions use this
    /// unfiltered set, including low-signal and unattributed destinations.
    pub fn unique_remote_ips(&self) -> Vec<IpAddr> {
        let mut set = HashSet::new();
        for app in self.apps.values() {
            for dest in app.destinations.values() {
                set.insert(dest.remote.ip());
            }
        }
        for dest in self.unattributed.destinations.values() {
            set.insert(dest.remote.ip());
        }
        let mut ips: Vec<_> = set.into_iter().collect();
        ips.sort();
        ips
    }
}

fn empty_entry(name: &str, path: Option<std::path::PathBuf>, id: &str, now: Instant) -> AppEntry {
    AppEntry {
        id: id.to_string(),
        name: name.to_string(),
        path,
        pids: BTreeSet::new(),
        processes: Default::default(),
        destinations: HashMap::new(),
        last_seen: now,
        hits_per_sec: 0.0,
        current_connections: 0,
        traffic: None,
    }
}

fn connection_display_name(conn: &Connection, key: &str) -> String {
    if !conn.process_name.is_empty() && conn.process_name != "unknown" {
        conn.process_name.clone()
    } else {
        key.to_string()
    }
}

/// Sticky identity: prefer full exe path (stable across renames of short name),
/// fall back to process name, then pid.
fn app_key(conn: &Connection) -> String {
    if let Some(id) = &conn.application_id {
        if !id.is_empty() {
            return id.clone();
        }
    }
    if let Some(path) = &conn.process_path {
        let s = path.to_string_lossy();
        if !s.is_empty() {
            return s.into_owned();
        }
    }
    if !conn.process_name.is_empty() && conn.process_name != "unknown" {
        return conn.process_name.clone();
    }
    match conn.pid {
        Some(pid) => format!("pid:{pid}"),
        None => "unknown".into(),
    }
}

/// Short display name for UI.
pub fn display_name_for(app: &AppEntry) -> String {
    if !app.name.is_empty() {
        let is_identity_path = app
            .path
            .as_ref()
            .is_some_and(|path| path.to_string_lossy() == app.name)
            || (app.name == app.id && (app.name.contains('/') || app.name.contains('\\')));
        if !is_identity_path {
            return app.name.clone();
        }
        if let Some(name) = std::path::Path::new(&app.name)
            .file_name()
            .and_then(|s| s.to_str())
        {
            return name.strip_suffix(".app").unwrap_or(name).to_string();
        }
    }
    if let Some(path) = &app.path {
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            return name.strip_suffix(".app").unwrap_or(name).to_string();
        }
    }
    app.name.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AttributionSource, ConnState, UnattributedReason};
    use std::net::SocketAddr;
    use std::path::PathBuf;

    fn connection(attribution: AttributionSource, is_new: bool) -> ConnectionObservation {
        let attributed = !matches!(attribution, AttributionSource::Unattributed);
        ConnectionObservation {
            active: true,
            connection: Connection {
                application_id: None,
                pid: attributed.then_some(42),
                process_name: if attributed {
                    "browser".into()
                } else {
                    String::new()
                },
                process_path: attributed.then(|| PathBuf::from("/usr/bin/browser")),
                processes: Vec::new(),
                local: "127.0.0.1:45123".parse::<SocketAddr>().unwrap(),
                remote: "203.0.113.10:443".parse::<SocketAddr>().unwrap(),
                protocol: Protocol::Tcp,
                state: ConnState::Established,
                attribution,
                unattributed_reason: (!attributed).then_some(UnattributedReason::OwnerGone),
                is_new,
                first_observed_at: Instant::now(),
                traffic_counters: None,
                destination_name: None,
            },
        }
    }

    #[test]
    fn repeated_socket_observations_count_once() {
        let mut state = AppState::new(Duration::from_secs(45), false);
        let mut hostnames = HostnameCache::new();
        state.ingest(
            vec![connection(AttributionSource::Direct, true)],
            &mut hostnames,
        );
        state.ingest(
            vec![connection(AttributionSource::Direct, false)],
            &mut hostnames,
        );

        let app = state.apps.get("/usr/bin/browser").unwrap();
        assert_eq!(app.connection_hits(), 1);
        assert_eq!(app.current_connections, 1);
        assert_eq!(app.hits_per_sec, 0.0);
    }

    #[test]
    fn recovered_owner_stays_with_application() {
        let mut state = AppState::new(Duration::from_secs(45), false);
        let mut hostnames = HostnameCache::new();
        state.ingest(
            vec![connection(AttributionSource::Recovered, false)],
            &mut hostnames,
        );

        assert_eq!(state.app_count(), 1);
        assert_eq!(state.attribution.recovered, 1);
        assert_eq!(state.missing_pid_count, 0);
    }

    #[test]
    fn local_destinations_are_always_hidden() {
        let mut state = AppState::new(Duration::from_secs(45), false);
        let mut hostnames = HostnameCache::new();
        let mut observation = connection(AttributionSource::Direct, true);
        observation.connection.remote = "192.168.1.1:443".parse().unwrap();

        state.ingest(vec![observation], &mut hostnames);

        assert_eq!(state.app_count(), 0);
        assert_eq!(state.last_raw_connections, 0);
    }

    #[test]
    fn friendly_app_name_wins_over_executable_path() {
        let mut state = AppState::new(Duration::from_secs(45), false);
        let mut hostnames = HostnameCache::new();
        let mut observation = connection(AttributionSource::Direct, true);
        observation.connection.application_id = Some("linux-desktop:app.zen_browser.zen".into());
        observation.connection.process_name = "Zen Browser".into();
        observation.connection.process_path = Some(PathBuf::from("/app/bin/zen"));
        state.ingest(vec![observation], &mut hostnames);

        let app = state.apps.get("linux-desktop:app.zen_browser.zen").unwrap();
        assert_eq!(display_name_for(app), "Zen Browser");
        assert_eq!(
            app.path.as_deref(),
            Some(std::path::Path::new("/app/bin/zen"))
        );
    }

    #[test]
    fn unattributed_traffic_is_not_an_application() {
        let mut state = AppState::new(Duration::from_secs(45), false);
        let mut hostnames = HostnameCache::new();
        state.ingest(
            vec![connection(AttributionSource::Unattributed, true)],
            &mut hostnames,
        );

        assert_eq!(state.app_count(), 0);
        assert_eq!(state.missing_pid_count, 1);
        assert_eq!(state.unattributed().connection_hits(), 1);
        assert_eq!(state.attribution.unattributed, 1);
        assert_eq!(state.attribution.owner_gone, 1);
    }

    #[test]
    fn cumulative_socket_bytes_become_per_app_rates_without_initial_spike() {
        let mut state = AppState::new(Duration::from_secs(45), false);
        let mut hostnames = HostnameCache::new();
        let mut conn = connection(AttributionSource::Direct, true);
        conn.connection.traffic_counters = Some(SocketTrafficCounters {
            rx_bytes: 100,
            tx_bytes: 200,
        });
        state.ingest(vec![conn.clone()], &mut hostnames);
        let first = state.apps.get("/usr/bin/browser").unwrap().traffic.unwrap();
        assert_eq!(first.total_bytes_per_sec(), 0.0);

        conn.connection.is_new = false;
        conn.connection.traffic_counters = Some(SocketTrafficCounters {
            rx_bytes: 300,
            tx_bytes: 500,
        });
        state.ingest(vec![conn], &mut hostnames);
        let second = state.apps.get("/usr/bin/browser").unwrap().traffic.unwrap();
        // Poll intervals are clamped to 200ms for stable activity rates.
        assert_eq!(second.rx_bytes_per_sec, 1_000.0);
        assert_eq!(second.tx_bytes_per_sec, 1_500.0);
    }

    #[test]
    fn historical_observation_updates_hits_but_not_live_counts() {
        let mut state = AppState::new(Duration::from_secs(45), false);
        let mut hostnames = HostnameCache::new();
        let mut observation = connection(AttributionSource::Unattributed, true);
        observation.active = false;
        observation.connection.state = ConnState::Closed;
        state.ingest(vec![observation], &mut hostnames);

        assert_eq!(state.last_raw_connections, 0);
        assert_eq!(state.unattributed().current_connections, 0);
        assert_eq!(state.unattributed().connection_hits(), 1);
        assert_eq!(state.missing_pid_count, 0);
    }

    #[test]
    fn one_off_attributed_destination_is_deferred_but_retained() {
        let mut state = AppState::new(Duration::from_secs(45), false);
        let mut hostnames = HostnameCache::new();
        state.ingest(
            vec![connection(AttributionSource::Direct, true)],
            &mut hostnames,
        );

        assert!(state.auto_trace_ips().is_empty());
        assert_eq!(
            state.unique_remote_ips(),
            vec!["203.0.113.10".parse::<IpAddr>().unwrap()]
        );
    }

    #[test]
    fn persistent_socket_qualifies_on_second_live_observation() {
        let mut state = AppState::new(Duration::from_secs(45), false);
        let mut hostnames = HostnameCache::new();
        let first = connection(AttributionSource::Direct, true);
        let mut second = first.clone();
        second.connection.is_new = false;

        state.ingest(vec![first], &mut hostnames);
        assert!(state.auto_trace_ips().is_empty());
        state.ingest(vec![second], &mut hostnames);

        assert_eq!(
            state.auto_trace_ips(),
            vec!["203.0.113.10".parse::<IpAddr>().unwrap()]
        );
    }

    #[test]
    fn repeated_short_connections_qualify_by_remote_ip() {
        let mut state = AppState::new(Duration::from_secs(45), false);
        let mut hostnames = HostnameCache::new();
        let mut first = connection(AttributionSource::Direct, true);
        first.active = false;
        let mut second = first.clone();
        second.connection.local = "127.0.0.1:45124".parse().unwrap();

        state.ingest(vec![first], &mut hostnames);
        assert!(state.auto_trace_ips().is_empty());
        state.ingest(vec![second], &mut hostnames);

        assert_eq!(
            state.auto_trace_ips(),
            vec!["203.0.113.10".parse::<IpAddr>().unwrap()]
        );
    }

    #[test]
    fn udp_only_destination_remains_deferred_despite_repeated_evidence() {
        let mut state = AppState::new(Duration::from_secs(45), true);
        let mut hostnames = HostnameCache::new();
        let mut first = connection(AttributionSource::Direct, true);
        first.connection.protocol = Protocol::Udp;
        let mut second = first.clone();
        second.connection.is_new = false;
        let third = second.clone();
        let fourth = second.clone();

        state.ingest(vec![first], &mut hostnames);
        assert!(state.auto_trace_ips().is_empty());
        state.ingest(vec![second], &mut hostnames);
        assert!(state.auto_trace_ips().is_empty());
        state.ingest(vec![third], &mut hostnames);
        assert!(state.auto_trace_ips().is_empty());
        state.ingest(vec![fourth], &mut hostnames);

        assert!(state.auto_trace_ips().is_empty());
    }

    #[test]
    fn unattributed_observations_never_supply_auto_trace_evidence() {
        let mut state = AppState::new(Duration::from_secs(45), false);
        let mut hostnames = HostnameCache::new();
        let first = connection(AttributionSource::Unattributed, true);
        let mut second = first.clone();
        second.connection.local = "127.0.0.1:45124".parse().unwrap();

        state.ingest(vec![first, second], &mut hostnames);

        assert!(state.auto_trace_ips().is_empty());
        assert_eq!(
            state.unique_remote_ips(),
            vec!["203.0.113.10".parse::<IpAddr>().unwrap()]
        );
    }
}
