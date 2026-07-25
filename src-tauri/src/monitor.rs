use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};

use crate::collect::{self, SniCache};
use crate::dto::{build_snapshot, PathChangedEvent, SettingsDto, SnapshotDto};
use crate::geo::{pending_ips, AsnDb, GeoCache, PathGeoCache};
use crate::model::{display_name_for, AppState};
use crate::resolve::HostnameCache;
use crate::settings_store;
use crate::trace::{TraceConfig, TraceEngine, TraceStatus};

pub struct Monitor {
    pub state: Mutex<AppState>,
    pub hostnames: Mutex<HostnameCache>,
    pub traces: Mutex<TraceEngine>,
    // These types provide their own interior synchronization. Keeping another
    // mutex around them made every snapshot wait for slow GeoIP HTTP calls.
    pub geo: GeoCache,
    pub path_geo: PathGeoCache,
    pub asn: AsnDb,
    pub settings: Mutex<SettingsDto>,
    /// Best-effort TLS SNI map (fed via `record_sni` / future pcap).
    pub sni: SniCache,
    /// Previous hop fingerprint per "app|ip"
    path_fps: Mutex<HashMap<String, u64>>,
    /// Keys that changed this cycle
    pub path_changed: Mutex<HashSet<String>>,
}

impl Monitor {
    pub fn new() -> Self {
        let mut settings = settings_store::load().unwrap_or_default();
        // Keep accepting the old setting on disk, but do not claim UDP support
        // until the collector can obtain remote UDP peers cross-platform.
        settings.include_udp = false;
        let retain = Duration::from_secs(45);
        let trace_cfg = TraceConfig {
            enabled: settings.traces_enabled,
            max_concurrent: 6,
            cache_ttl: Duration::from_secs(900),
            max_hops: 20,
            process_timeout: Duration::from_secs(28),
            skip_private: true,
        };

        Self {
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
            sni: SniCache::new(),
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

        let connections = collect::snapshot(settings.include_udp).map_err(|e| e.to_string())?;

        {
            let mut state = self.state.lock();
            let mut hostnames = self.hostnames.lock();
            state.ingest(connections, &mut hostnames);
            if settings.capture_sni {
                self.sni.apply_to_state(&mut state);
            }
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
        let state = self.state.lock();
        let settings = self.settings.lock().clone();
        let changed = self.path_changed.lock().clone();
        build_snapshot(
            &state,
            &traces,
            &self.geo,
            &self.path_geo,
            &self.asn,
            &settings,
            &changed,
        )
    }

    pub fn apply_settings(&self, mut settings: SettingsDto) {
        settings.include_udp = false;
        let traces_changed = {
            let mut current = self.settings.lock();
            let changed = current.traces_enabled != settings.traces_enabled;
            *current = settings.clone();
            changed
        };
        let _ = settings_store::save(&settings);
        {
            let mut state = self.state.lock();
            state.external_only = settings.external_only;
            state.include_udp = settings.include_udp;
        }
        if traces_changed {
            let cfg = TraceConfig {
                enabled: settings.traces_enabled,
                max_concurrent: 6,
                cache_ttl: Duration::from_secs(900),
                max_hops: 20,
                process_timeout: Duration::from_secs(28),
                skip_private: true,
            };
            *self.traces.lock() = TraceEngine::new(cfg);
            self.path_geo.clear();
        }
    }

    pub fn reset(&self) {
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

pub fn spawn_poll_loop(app: AppHandle, monitor: Arc<Monitor>) {
    {
        let mon = Arc::clone(&monitor);
        thread::Builder::new()
            .name("geo-warm".into())
            .spawn(move || loop {
                let settings = mon.settings.lock().clone();
                if !settings.privacy_accepted {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                let pending = mon.pending_geo_ips();
                if pending.is_empty() {
                    thread::sleep(Duration::from_secs(2));
                    continue;
                }
                let batch: Vec<IpAddr> = pending.into_iter().take(40).collect();
                mon.geo.resolve_batch(&batch, settings.geo_local_only);
                mon.path_geo.clear();
                thread::sleep(Duration::from_millis(600));
            })
            .expect("spawn geo warmer");
    }

    // optional history writer
    {
        let mon = Arc::clone(&monitor);
        let app_h = app.clone();
        thread::Builder::new()
            .name("history".into())
            .spawn(move || loop {
                thread::sleep(Duration::from_secs(60));
                let enabled = mon.settings.lock().history_enabled;
                if !enabled {
                    continue;
                }
                let snap = mon.snapshot();
                if let Err(e) = crate::history::append_snapshot(&snap) {
                    let _ = app_h.emit("monitor-error", format!("history: {e}"));
                }
            })
            .expect("spawn history");
    }

    thread::Builder::new()
        .name("monitor-poll".into())
        .spawn(move || {
            let mut last_socket_poll = Instant::now() - Duration::from_secs(10);
            let mut last_geo_emit = Instant::now() - Duration::from_secs(10);
            let mut last_pending_geo = 0usize;
            loop {
                let interval_ms = monitor.settings.lock().poll_interval_ms.max(500);
                let interval = Duration::from_millis(interval_ms);

                if last_socket_poll.elapsed() >= interval {
                    match monitor.tick() {
                        Ok(snap) => {
                            let _ = app.emit("monitor-update", &snap);
                            for ev in monitor.drain_path_change_events() {
                                let _ = app.emit("path-changed", &ev);
                            }
                        }
                        Err(e) => {
                            let _ = app.emit("monitor-error", e);
                        }
                    }
                    last_socket_poll = Instant::now();
                    last_geo_emit = Instant::now();
                    last_pending_geo = monitor.pending_geo_ips().len();
                } else {
                    let pending = monitor.pending_geo_ips().len();
                    let geo_progressed = pending < last_pending_geo;
                    if (geo_progressed || pending > 0)
                        && last_geo_emit.elapsed() >= Duration::from_millis(1500)
                    {
                        let snap = monitor.snapshot();
                        let _ = app.emit("monitor-update", &snap);
                        last_geo_emit = Instant::now();
                        last_pending_geo = pending;
                    }
                }

                thread::sleep(Duration::from_millis(350));
            }
        })
        .expect("spawn monitor poll loop");
}
