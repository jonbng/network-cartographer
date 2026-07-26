use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::time::Duration;

use parking_lot::Mutex;

use crate::collect::{
    DestinationNameCache, DestinationNamingStatus, OsDnsCollector, SniObservation, SocketCollector,
};
use crate::dto::PathChangedEvent;
use crate::dto::{build_snapshot, SettingsDto, SnapshotContext, SnapshotDto};
use crate::geo::{pending_ips, AsnDb, GeoCache, PathGeoCache};
use crate::model::{display_name_for, AppState};
use crate::resolve::HostnameCache;
use crate::settings_store;
use crate::trace::{TraceConfig, TraceEngine, TraceStatus};

pub struct Monitor {
    pub collector: Mutex<SocketCollector>,
    pub state: Mutex<AppState>,
    pub hostnames: Mutex<HostnameCache>,
    pub traces: Mutex<TraceEngine>,
    // These types provide their own interior synchronization. Keeping another
    // mutex around them made every snapshot wait for slow GeoIP HTTP calls.
    pub geo: GeoCache,
    pub path_geo: PathGeoCache,
    pub asn: AsnDb,
    pub settings: Mutex<SettingsDto>,
    pub destination_names: DestinationNameCache,
    dns_collector: Mutex<OsDnsCollector>,
    /// Previous hop fingerprint per "app|ip"
    path_fps: Mutex<HashMap<String, u64>>,
    /// Keys that changed this cycle
    pub path_changed: Mutex<HashSet<String>>,
}

impl Monitor {
    pub fn new() -> Self {
        let settings = settings_store::load().unwrap_or_default();
        let retain = Duration::from_secs(45);
        let trace_cfg = TraceConfig {
            enabled: settings.traces_enabled,
            ..TraceConfig::default()
        };

        Self {
            collector: Mutex::new(SocketCollector::default()),
            state: Mutex::new(AppState::new(
                retain,
                settings.external_only,
                settings.include_udp,
            )),
            hostnames: Mutex::new(HostnameCache::new()),
            traces: Mutex::new(TraceEngine::new(trace_cfg)),
            geo: GeoCache::new(),
            path_geo: PathGeoCache::new(),
            asn: AsnDb::new(),
            settings: Mutex::new(settings),
            destination_names: DestinationNameCache::new(),
            dns_collector: Mutex::new(OsDnsCollector::default()),
            path_fps: Mutex::new(HashMap::new()),
            path_changed: Mutex::new(HashSet::new()),
        }
    }

    pub fn tick(&self) -> Result<SnapshotDto, String> {
        let settings = self.settings.lock().clone();
        // The first-run notice promises that monitoring and external lookups do
        // not start until the user accepts it.
        if !settings.privacy_accepted {
            return Ok(self.snapshot());
        }
        {
            let mut state = self.state.lock();
            state.external_only = settings.external_only;
            state.include_udp = settings.include_udp;
        }

        let mut connections = self
            .collector
            .lock()
            .snapshot(settings.include_udp, settings.enhanced_monitoring)
            .map_err(|e| e.to_string())?;

        if settings.identify_domains {
            self.destination_names.enrich(&mut connections);
        }

        {
            let mut state = self.state.lock();
            let mut hostnames = self.hostnames.lock();
            state.ingest(connections, &mut hostnames);
        }

        {
            let mut traces = self.traces.lock();
            if traces.enabled() {
                let ips = self.state.lock().unique_remote_ips();
                for ip in ips {
                    traces.request(ip);
                }
            }
            traces.poll();
        }

        self.detect_path_changes();
        Ok(self.snapshot())
    }

    pub fn snapshot(&self) -> SnapshotDto {
        // Keep the same traces -> state lock order used by tick(),
        // pending_geo_ips(), and detect_path_changes().
        let mut traces = self.traces.lock();
        traces.poll();
        let destination_naming = self.destination_naming_status();
        let state = self.state.lock();
        let settings = self.settings.lock().clone();
        let collector = self.collector.lock();
        let traffic_status = collector.traffic_status();
        let collection_status = collector.collection_status();
        let udp_status = collector.udp_status();
        drop(collector);
        let changed = self.path_changed.lock().clone();
        build_snapshot(
            &state,
            &SnapshotContext {
                traces: &traces,
                geo: &self.geo,
                path_geo: &self.path_geo,
                asn: &self.asn,
                settings: &settings,
                path_changed: &changed,
                traffic_status: &traffic_status,
                collection_status: &collection_status,
                destination_naming: &destination_naming,
                udp_status: &udp_status,
            },
        )
    }

    pub fn collection_events_pending(&self) -> bool {
        self.collector.lock().events_pending()
    }

    pub fn poll_domain_sources(&self) {
        let settings = self.settings.lock().clone();
        if settings.privacy_accepted && settings.identify_domains {
            self.dns_collector.lock().poll(&self.destination_names);
        }
    }

    pub fn destination_naming_status(&self) -> DestinationNamingStatus {
        if !self.settings.lock().identify_domains {
            return DestinationNamingStatus {
                status: "disabled",
                sources: Vec::new(),
                message: "Destination domain identification is disabled".into(),
            };
        }
        self.dns_collector.lock().status()
    }

    pub fn record_sni(&self, observation: SniObservation) -> Result<(), String> {
        if !self.settings.lock().identify_domains {
            return Err("destination domain identification is disabled".into());
        }
        self.destination_names.record_sni(observation)
    }

    pub fn apply_settings(&self, mut settings: SettingsDto) {
        settings.settings_version = 2;
        let (traces_changed, domains_disabled) = {
            let mut current = self.settings.lock();
            let changed = current.traces_enabled != settings.traces_enabled;
            let domains_disabled = current.identify_domains && !settings.identify_domains;
            *current = settings.clone();
            (changed, domains_disabled)
        };
        let _ = settings_store::save(&settings);
        {
            let mut state = self.state.lock();
            state.external_only = settings.external_only;
            state.include_udp = settings.include_udp;
        }
        if domains_disabled {
            self.destination_names.clear();
            self.state.lock().clear_destination_names();
        }
        if traces_changed {
            let cfg = TraceConfig {
                enabled: settings.traces_enabled,
                ..TraceConfig::default()
            };
            *self.traces.lock() = TraceEngine::new(cfg);
            self.path_geo.clear();
        }
    }

    pub fn reset(&self) {
        self.collector.lock().reset();
        self.destination_names.clear();
        self.state.lock().reset();
        self.traces.lock().clear_cache();
        self.path_geo.clear();
        self.path_fps.lock().clear();
        self.path_changed.lock().clear();
    }

    pub fn pending_geo_ips(&self) -> Vec<IpAddr> {
        let traces = self.traces.lock();
        let state = self.state.lock();
        let mut set = HashSet::new();

        for app in state.sorted_apps() {
            for dest in app.destinations.values() {
                if let TraceStatus::Done(r) = traces.get(dest.remote.ip()) {
                    for ip in pending_ips(&r.hops, &self.geo) {
                        set.insert(ip);
                    }
                    if self.geo.needs_resolve(dest.remote.ip()) {
                        set.insert(dest.remote.ip());
                    }
                }
            }
        }
        set.into_iter().collect()
    }

    fn detect_path_changes(&self) {
        let traces = self.traces.lock();
        let state = self.state.lock();
        let mut fps = self.path_fps.lock();
        let mut changed = self.path_changed.lock();
        changed.clear();

        for app in state.sorted_apps() {
            let app_name = display_name_for(app);
            for dest in app.destinations.values() {
                if let TraceStatus::Done(r) = traces.get(dest.remote.ip()) {
                    let key = format!("{}|{}", app_name, dest.remote.ip());
                    let fp = hop_fingerprint(&r.hops);
                    if let Some(prev) = fps.get(&key) {
                        if *prev != fp && fp != 0 {
                            changed.insert(key.clone());
                        }
                    }
                    fps.insert(key, fp);
                }
            }
        }
    }

    pub fn drain_path_change_events(&self) -> Vec<PathChangedEvent> {
        let changed = self.path_changed.lock().clone();
        let state = self.state.lock();
        let mut out = Vec::new();
        for key in changed {
            let mut parts = key.splitn(2, '|');
            let app = parts.next().unwrap_or("?").to_string();
            let ip = parts.next().unwrap_or("?").to_string();
            let host = state
                .sorted_apps()
                .iter()
                .find(|a| display_name_for(a) == app)
                .and_then(|a| {
                    a.destinations
                        .values()
                        .find(|d| d.remote.ip().to_string() == ip)
                        .map(|d| d.display_host())
                })
                .unwrap_or_else(|| ip.clone());
            out.push(PathChangedEvent {
                app,
                host,
                ip,
                summary: "traceroute path changed".into(),
            });
        }
        out
    }
}

fn hop_fingerprint(hops: &[crate::trace::Hop]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for hop in hops {
        hop.ttl.hash(&mut h);
        hop.addr.hash(&mut h);
    }
    h.finish()
}
