//! Shared frontend data model and live monitor bridge.

use network_cartographer_backend::standalone::{SettingsDto, SnapshotDto, StandaloneMonitor};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub apps: Vec<Application>,
    pub app_count: usize,
    pub destination_count: usize,
    pub live_connections: usize,
    pub missing_pid: usize,
    pub trace_stats: TraceStats,
    pub geo_backend: String,
    pub settings: Settings,
    pub demo: bool,
}

#[derive(Clone, Debug, Default)]
pub struct TraceStats {
    pub queued: usize,
    pub running: usize,
    pub done: usize,
    pub failed: usize,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub traces_enabled: bool,
    pub geo_local_only: bool,
    pub history_enabled: bool,
    pub privacy_accepted: bool,
    pub density: Density,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            traces_enabled: true,
            geo_local_only: false,
            history_enabled: false,
            privacy_accepted: false,
            density: Density::All,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Density {
    #[default]
    All,
    Destinations,
    Hubs,
}

impl Density {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Destinations,
            Self::Destinations => Self::Hubs,
            Self::Hubs => Self::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all hops",
            Self::Destinations => "destinations",
            Self::Hubs => "hubs + dests",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Application {
    pub id: String,
    pub name: String,
    pub activity: f64,
    pub destinations: Vec<Destination>,
}

#[derive(Clone, Debug)]
pub struct Destination {
    pub id: String,
    pub host: String,
    pub ip: String,
    pub port: u16,
    pub protocol: String,
    pub hits: u64,
    pub org: Option<String>,
    pub path_changed: bool,
    pub status: String,
    pub rtt_ms: Option<f64>,
    pub hops: Vec<Hop>,
}

#[derive(Clone, Debug)]
pub struct Hop {
    pub ttl: u8,
    pub addr: Option<String>,
    pub lat: Option<f32>,
    pub lon: Option<f32>,
    pub city: Option<String>,
    pub country: Option<String>,
}

impl Snapshot {
    pub fn mapped_path_count(&self) -> usize {
        self.apps
            .iter()
            .flat_map(|app| &app.destinations)
            .filter(|dest| {
                dest.hops
                    .iter()
                    .any(|hop| hop.lat.is_some() && hop.lon.is_some())
            })
            .count()
    }

    pub fn mapped_hop_count(&self) -> usize {
        self.apps
            .iter()
            .flat_map(|app| &app.destinations)
            .map(|dest| {
                dest.hops
                    .iter()
                    .filter(|hop| hop.lat.is_some() && hop.lon.is_some())
                    .count()
            })
            .sum()
    }
}

impl From<SnapshotDto> for Snapshot {
    fn from(dto: SnapshotDto) -> Self {
        let settings = Settings::from_dto(&dto.settings);
        let apps = dto
            .apps
            .into_iter()
            .map(|app| {
                let app_id = app.id;
                let destinations = app
                    .destinations
                    .into_iter()
                    .map(|dest| {
                        let rtt_ms = dest.trace.hops.iter().rev().find_map(|hop| hop.rtt_ms);
                        let id = format!("{}|{}|{}", app_id, dest.ip, dest.port);
                        Destination {
                            id,
                            host: if dest.display_host.is_empty() {
                                dest.host
                            } else {
                                dest.display_host
                            },
                            ip: dest.ip,
                            port: dest.port,
                            protocol: dest.protocol,
                            hits: dest.hits,
                            org: dest.org,
                            path_changed: dest.path_changed,
                            status: dest.trace.status,
                            rtt_ms,
                            hops: dest
                                .trace
                                .hops
                                .into_iter()
                                .map(|hop| Hop {
                                    ttl: hop.ttl,
                                    addr: hop.hostname.or(hop.addr),
                                    lat: hop.lat.map(|v| v as f32),
                                    lon: hop.lon.map(|v| v as f32),
                                    city: hop.city,
                                    country: hop.country,
                                })
                                .collect(),
                        }
                    })
                    .collect();
                Application {
                    id: app_id,
                    name: app.name,
                    activity: app.activity,
                    destinations,
                }
            })
            .collect();

        Self {
            apps,
            app_count: dto.app_count,
            destination_count: dto.dest_count,
            live_connections: dto.live_connections,
            missing_pid: dto.missing_pid,
            trace_stats: TraceStats {
                queued: dto.trace_stats.queued,
                running: dto.trace_stats.running,
                done: dto.trace_stats.done,
                failed: dto.trace_stats.failed,
            },
            geo_backend: dto.geo_backend,
            settings,
            demo: false,
        }
    }
}

impl Settings {
    fn from_dto(dto: &SettingsDto) -> Self {
        Self {
            traces_enabled: dto.traces_enabled,
            geo_local_only: dto.geo_local_only,
            history_enabled: dto.history_enabled,
            privacy_accepted: dto.privacy_accepted,
            density: match dto.globe_density.as_str() {
                "destinations" => Density::Destinations,
                "hubs" => Density::Hubs,
                _ => Density::All,
            },
        }
    }

    fn apply_to_dto(&self, dto: &mut SettingsDto) {
        dto.traces_enabled = self.traces_enabled;
        dto.geo_local_only = self.geo_local_only;
        dto.history_enabled = self.history_enabled;
        dto.privacy_accepted = self.privacy_accepted;
        dto.globe_density = match self.density {
            Density::All => "all",
            Density::Destinations => "destinations",
            Density::Hubs => "hubs",
        }
        .into();
    }
}

#[derive(Debug)]
pub enum SourceCommand {
    ApplySettings(Settings),
    TraceAll,
    Reset,
}

#[derive(Debug)]
pub enum SourceEvent {
    Snapshot(Snapshot),
    Error(String),
}

pub struct SourceHandle {
    pub events: Receiver<SourceEvent>,
    pub commands: Sender<SourceCommand>,
    stop: Arc<AtomicBool>,
}

impl Drop for SourceHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn spawn_source(demo: bool) -> SourceHandle {
    let (event_tx, event_rx) = mpsc::channel();
    let (command_tx, command_rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));

    if demo {
        let stop_worker = Arc::clone(&stop);
        thread::Builder::new()
            .name("cartographer-demo".into())
            .spawn(move || {
                let _ = event_tx.send(SourceEvent::Snapshot(crate::mock::demo_snapshot()));
                while !stop_worker.load(Ordering::Relaxed) {
                    if let Ok(command) = command_rx.recv_timeout(Duration::from_millis(250)) {
                        if matches!(command, SourceCommand::Reset) {
                            let _ =
                                event_tx.send(SourceEvent::Snapshot(crate::mock::demo_snapshot()));
                        }
                    }
                }
            })
            .expect("spawn demo data source");
        return SourceHandle {
            events: event_rx,
            commands: command_tx,
            stop,
        };
    }

    let monitor = StandaloneMonitor::new();
    let geo_monitor = monitor.clone();
    let geo_stop = Arc::clone(&stop);
    thread::Builder::new()
        .name("cartographer-cli-geo".into())
        .spawn(move || {
            while !geo_stop.load(Ordering::Relaxed) {
                let resolved = geo_monitor.warm_geo_once(40);
                let wait = if resolved > 0 { 600 } else { 1_500 };
                thread::sleep(Duration::from_millis(wait));
            }
        })
        .expect("spawn cli geo warmer");

    let stop_worker = Arc::clone(&stop);
    thread::Builder::new()
        .name("cartographer-cli-monitor".into())
        .spawn(move || {
            let _ = event_tx.send(SourceEvent::Snapshot(monitor.snapshot().into()));
            let mut last_tick = Instant::now() - Duration::from_secs(10);
            let mut last_history = Instant::now();
            while !stop_worker.load(Ordering::Relaxed) {
                while let Ok(command) = command_rx.try_recv() {
                    match command {
                        SourceCommand::ApplySettings(settings) => {
                            let mut dto = monitor.settings();
                            settings.apply_to_dto(&mut dto);
                            let snapshot = monitor.apply_settings(dto);
                            let mut current = monitor.snapshot();
                            current.settings = snapshot;
                            let _ = event_tx.send(SourceEvent::Snapshot(current.into()));
                        }
                        SourceCommand::TraceAll => monitor.force_trace_all(),
                        SourceCommand::Reset => monitor.reset(),
                    }
                }

                let interval = Duration::from_millis(monitor.settings().poll_interval_ms.max(500));
                if last_tick.elapsed() >= interval {
                    match monitor.tick() {
                        Ok(snapshot) => {
                            let _ = event_tx.send(SourceEvent::Snapshot(snapshot.into()));
                        }
                        Err(error) => {
                            let _ = event_tx.send(SourceEvent::Error(error));
                        }
                    }
                    last_tick = Instant::now();
                }
                if monitor.settings().history_enabled
                    && last_history.elapsed() >= Duration::from_secs(60)
                {
                    if let Err(error) = monitor.append_history() {
                        let _ = event_tx.send(SourceEvent::Error(format!("history: {error}")));
                    }
                    last_history = Instant::now();
                }
                thread::sleep(Duration::from_millis(100));
            }
        })
        .expect("spawn cli monitor");

    SourceHandle {
        events: event_rx,
        commands: command_tx,
        stop,
    }
}

pub fn color_for_key(key: &str) -> [u8; 3] {
    const PALETTE: [[u8; 3]; 14] = [
        [34, 211, 238],
        [167, 139, 250],
        [52, 211, 153],
        [244, 114, 182],
        [251, 191, 36],
        [96, 165, 250],
        [251, 113, 133],
        [45, 212, 191],
        [192, 132, 252],
        [74, 222, 128],
        [56, 189, 248],
        [232, 121, 249],
        [249, 115, 22],
        [20, 184, 166],
    ];
    let hash = key.bytes().fold(0u32, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(byte as u32)
    });
    PALETTE[hash as usize % PALETTE.len()]
}
