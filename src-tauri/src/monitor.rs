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
    pub geo: Mutex<GeoCache>,
    pub path_geo: Mutex<PathGeoCache>,
    pub asn: Mutex<AsnDb>,
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
        let settings = settings_store::load().unwrap_or_default();
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
            geo: Mutex::new(GeoCache::new()),
            path_geo: Mutex::new(PathGeoCache::new()),
            asn: Mutex::new(AsnDb::new()),
            settings: Mutex::new(settings),
            sni: SniCache::new(),
            path_fps: Mutex::new(HashMap::new()),
            path_changed: Mutex::new(HashSet::new()),
        }
    }

    pub fn tick(&self) -> Result<SnapshotDto, String> {
        let settings = self.settings.lock().clone();
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
        {
            let mut traces = self.traces.lock();
            traces.poll();
        }
        let state = self.state.lock();
        let traces = self.traces.lock();
        let geo = self.geo.lock();
        let path_geo = self.path_geo.lock();
        let asn = self.asn.lock();
        let settings = self.settings.lock().clone();
        let changed = self.path_changed.lock().clone();
        build_snapshot(
            &state,
            &traces,
            &geo,
            &path_geo,
            &asn,
            &settings,
            &changed,
        )
    }

    pub fn apply_settings(&self, settings: SettingsDto) {
        let mut s = self.settings.lock();
        let traces_changed = s.traces_enabled != settings.traces_enabled;
        *s = settings.clone();
        let _ = settings_store::save(&settings);
        let mut state = self.state.lock();
        state.external_only = s.external_only;
        state.include_udp = s.include_udp;
        if traces_changed {
            let cfg = TraceConfig {
                enabled: s.traces_enabled,
                max_concurrent: 6,
                cache_ttl: Duration::from_secs(900),
                max_hops: 20,
                process_timeout: Duration::from_secs(28),
                skip_private: true,
            };
            *self.traces.lock() = TraceEngine::new(cfg);
            self.path_geo.lock().clear();
        }
    }

    pub fn reset(&self) {
        self.state.lock().reset();
        self.traces.lock().clear_cache();
        self.path_geo.lock().clear();
        self.path_fps.lock().clear();
        self.path_changed.lock().clear();
    }

    pub fn pending_geo_ips(&self) -> Vec<IpAddr> {
        let traces = self.traces.lock();
        let geo = self.geo.lock();
        let state = self.state.lock();
        let mut set = HashSet::new();

        for app in state.sorted_apps() {
            for dest in app.destinations.values() {
                if let TraceStatus::Done(r) = traces.get(dest.remote.ip()) {
                    for ip in pending_ips(&r.hops, &geo) {
                        set.insert(ip);
                    }
                    if geo.needs_resolve(dest.remote.ip()) {
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
                let pending = mon.pending_geo_ips();
                if pending.is_empty() {
                    thread::sleep(Duration::from_secs(2));
                    continue;
                }
                let local_only = mon.settings.lock().geo_local_only;
                let batch: Vec<IpAddr> = pending.into_iter().take(40).collect();
                mon.geo.lock().resolve_batch(&batch, local_only);
                mon.path_geo.lock().clear();
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
