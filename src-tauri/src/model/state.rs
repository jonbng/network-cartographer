use std::collections::{BTreeSet, HashMap, HashSet};
use std::net::IpAddr;
use std::time::{Duration, Instant};

use crate::resolve::HostnameCache;

use super::{
    is_local_or_private, AppEntry, Connection, DestKey, DestStats, Protocol,
};

#[derive(Debug)]
pub struct AppState {
    apps: HashMap<String, AppEntry>,
    pub retain: Duration,
    pub external_only: bool,
    pub include_udp: bool,
    pub missing_pid_count: usize,
    pub last_poll_at: Option<Instant>,
    pub poll_count: u64,
    /// Snapshot connection count from last poll (after filters).
    pub last_raw_connections: usize,
}

impl AppState {
    pub fn new(retain: Duration, external_only: bool, include_udp: bool) -> Self {
        Self {
            apps: HashMap::new(),
            retain,
            external_only,
            include_udp,
            missing_pid_count: 0,
            last_poll_at: None,
            poll_count: 0,
            last_raw_connections: 0,
        }
    }

    pub fn reset(&mut self) {
        self.apps.clear();
        self.missing_pid_count = 0;
        self.poll_count = 0;
        self.last_raw_connections = 0;
        self.last_poll_at = None;
    }

    pub fn ingest(&mut self, connections: Vec<Connection>, hostnames: &mut HostnameCache) {
        let now = Instant::now();
        let dt = self
            .last_poll_at
            .map(|t| now.duration_since(t).as_secs_f64().max(0.2))
            .unwrap_or(1.0);
        self.last_poll_at = Some(now);
        self.poll_count += 1;

        let mut missing = 0usize;
        let mut kept = 0usize;

        for conn in connections {
            if matches!(conn.protocol, Protocol::Udp) && !self.include_udp {
                continue;
            }
            // Skip pure listeners with no remote peer
            if conn.remote.ip().is_unspecified() || conn.remote.port() == 0 {
                continue;
            }
            if self.external_only && is_local_or_private(conn.remote.ip()) {
                continue;
            }
            if conn.pid.is_none() {
                missing += 1;
            }

            kept += 1;
            hostnames.request(conn.remote.ip());

            let key = app_key(&conn);
            let display = if !conn.process_name.is_empty() && conn.process_name != "unknown" {
                conn.process_name.clone()
            } else {
                key.clone()
            };
            let entry = self.apps.entry(key).or_insert_with(|| AppEntry {
                name: display,
                path: conn.process_path.clone(),
                pids: BTreeSet::new(),
                destinations: HashMap::new(),
                last_seen: now,
                hits_prev_cycle: 0,
                hits_per_sec: 0.0,
            });

            entry.last_seen = now;
            if let Some(pid) = conn.pid {
                entry.pids.insert(pid);
            }
            if entry.path.is_none() {
                entry.path = conn.process_path.clone();
            }
            // Prefer a real short name when we learn one
            if (entry.name == "unknown" || entry.name.starts_with("pid:"))
                && !conn.process_name.is_empty()
                && conn.process_name != "unknown"
            {
                entry.name = conn.process_name.clone();
            }

            let dest_key = DestKey::from_remote(conn.remote, conn.protocol);
            let hostname = hostnames.get(conn.remote.ip());
            let dest = entry.destinations.entry(dest_key).or_insert_with(|| DestStats {
                remote: conn.remote,
                hostname: hostname.clone(),
                sni: None,
                protocol: conn.protocol,
                hit_count: 0,
                first_seen: now,
                last_seen: now,
            });
            dest.hit_count += 1;
            dest.last_seen = now;
            dest.protocol = conn.protocol;
            if dest.hostname.is_none() {
                dest.hostname = hostname;
            } else if let Some(h) = hostname {
                dest.hostname = Some(h);
            }
        }

        self.missing_pid_count = missing;
        self.last_raw_connections = kept;

        // Age out stale destinations / apps
        let retain = self.retain;
        self.apps.retain(|_, app| {
            app.destinations
                .retain(|_, d| now.duration_since(d.last_seen) <= retain);
            !app.destinations.is_empty() && now.duration_since(app.last_seen) <= retain
        });

        // Refresh hostnames + activity rates
        for app in self.apps.values_mut() {
            let hits = app.connection_hits();
            let delta = hits.saturating_sub(app.hits_prev_cycle);
            app.hits_per_sec = delta as f64 / dt;
            app.hits_prev_cycle = hits;
            for dest in app.destinations.values_mut() {
                if let Some(h) = hostnames.get(dest.remote.ip()) {
                    dest.hostname = Some(h);
                }
            }
        }
    }

    pub fn apply_sni(&mut self, remote: IpAddr, sni: &str) {
        for app in self.apps.values_mut() {
            for dest in app.destinations.values_mut() {
                if dest.remote.ip() == remote {
                    dest.sni = Some(sni.to_string());
                }
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

    pub fn filtered_apps(&self, filter: &str) -> Vec<&AppEntry> {
        let filter = filter.trim().to_lowercase();
        if filter.is_empty() {
            return self.sorted_apps();
        }
        self.sorted_apps()
            .into_iter()
            .filter(|app| {
                if app.name.to_lowercase().contains(&filter) {
                    return true;
                }
                app.destinations.values().any(|d| {
                    d.display_host().to_lowercase().contains(&filter)
                        || d.remote.ip().to_string().contains(&filter)
                })
            })
            .collect()
    }

    pub fn total_destinations(&self) -> usize {
        self.apps.values().map(|a| a.destinations.len()).sum()
    }

    pub fn app_count(&self) -> usize {
        self.apps.len()
    }

    /// Unique remote IPs currently retained (for traceroute enqueue).
    pub fn unique_remote_ips(&self) -> Vec<IpAddr> {
        let mut set = HashSet::new();
        for app in self.apps.values() {
            for dest in app.destinations.values() {
                set.insert(dest.remote.ip());
            }
        }
        let mut ips: Vec<_> = set.into_iter().collect();
        ips.sort();
        ips
    }
}

/// Sticky identity: prefer full exe path (stable across renames of short name),
/// fall back to process name, then pid.
fn app_key(conn: &Connection) -> String {
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
    if let Some(path) = &app.path {
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            return name.to_string();
        }
    }
    // key may be full path
    if app.name.contains('/') || app.name.contains('\\') {
        return std::path::Path::new(&app.name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&app.name)
            .to_string();
    }
    app.name.clone()
}
