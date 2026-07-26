use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::collect::{
    CollectionStatus, DestinationNamingStatus, NativeTrafficStatus, UdpCollectionStatus,
};
use crate::geo::{AsnDb, GeoCache, GeoHop, PathGeoCache};
use crate::model::{display_name_for, AppEntry, AppState, DestStats};
use crate::network_origin::NetworkOriginView;
use crate::trace::{TraceEngine, TraceStatus};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDto {
    pub apps: Vec<AppDto>,
    pub app_count: usize,
    pub dest_count: usize,
    pub live_connections: usize,
    pub missing_pid: usize,
    pub attribution: AttributionStatsDto,
    pub unattributed: Option<TrafficGroupDto>,
    pub monitoring: MonitoringDto,
    pub collection: CollectionDto,
    pub destination_naming: DestinationNamingDto,
    pub udp_monitoring: UdpMonitoringDto,
    pub external_only: bool,
    pub include_udp: bool,
    pub traces_enabled: bool,
    pub trace_stats: TraceStatsDto,
    pub geo_backend: String,
    pub geo_mmdb: bool,
    pub geo_asn_mmdb: bool,
    pub settings: SettingsDto,
    pub network_origin: NetworkOriginDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkOriginDto {
    pub status: String,
    pub exit: Option<NetworkExitDto>,
    pub assessment: String,
    pub evidence: Vec<NetworkOriginEvidenceDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkExitDto {
    pub ip: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub asn: Option<u32>,
    pub organization: Option<String>,
    pub source: String,
    pub confidence: Option<f64>,
    pub age_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkOriginEvidenceDto {
    pub kind: String,
    pub strength: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceStatsDto {
    pub queued: usize,
    pub running: usize,
    pub done: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDto {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    pub pids: Vec<u32>,
    pub processes: Vec<ProcessDto>,
    pub dest_count: usize,
    pub hits: u64,
    pub hits_per_sec: f64,
    pub activity: f64,
    pub current_connections: usize,
    pub new_connections_per_sec: f64,
    pub traffic: Option<TrafficRateDto>,
    pub destinations: Vec<DestDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessDto {
    pub id: String,
    pub pid: u32,
    pub start_time: u64,
    pub name: String,
    pub path: Option<String>,
    pub parent_pid: Option<u32>,
    pub is_app_root: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficGroupDto {
    pub name: String,
    pub current_connections: usize,
    pub connections: u64,
    pub destinations: Vec<DestDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficRateDto {
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub total_bytes_per_sec: f64,
    pub sample_window_ms: u64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributionStatsDto {
    pub direct: usize,
    pub recovered: usize,
    pub unattributed: usize,
    pub ambiguous: usize,
    pub owner_gone: usize,
    pub access_limited: usize,
    pub ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringDto {
    pub mode: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDto {
    pub mode: String,
    pub source: String,
    pub captures_opens: bool,
    pub captures_closes: bool,
    pub dropped_events: u64,
    pub status: String,
    pub message: String,
    pub udp_remote: bool,
    pub access_limited: usize,
    pub poll_phase: String,
    pub effective_poll_interval_ms: u64,
    pub observed_opens: u64,
    pub observed_closes: u64,
    pub recovered_owners: u64,
    pub unattributed_owner_gone: u64,
    pub unattributed_ambiguous: u64,
    pub unattributed_access_limited: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationNamingDto {
    pub enabled: bool,
    pub status: String,
    pub sources: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UdpMonitoringDto {
    pub enabled: bool,
    pub coverage: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestDto {
    pub host: String,
    pub display_host: String,
    pub ip: String,
    pub port: u16,
    pub protocol: String,
    pub hits: u64,
    pub last_seen_secs: u64,
    pub sni: Option<String>,
    pub domain: Option<String>,
    pub domain_source: String,
    pub domain_confidence: String,
    pub domain_alternatives_count: usize,
    pub process_ids: Vec<String>,
    pub asn: Option<u32>,
    pub org: Option<String>,
    pub path_changed: bool,
    pub trace: TraceDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceDto {
    pub status: String,
    pub label: String,
    pub hops: Vec<HopDto>,
    pub error: Option<String>,
    pub reached_target: bool,
    pub target_rtt_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HopDto {
    pub ttl: u8,
    pub addr: Option<String>,
    pub rtt_ms: Option<f64>,
    pub hostname: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub geo_source: Option<String>,
    pub geo_confidence: Option<f64>,
    pub geo_note: Option<String>,
    pub asn: Option<u32>,
    pub org: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsDto {
    pub settings_version: u32,
    pub external_only: bool,
    pub include_udp: bool,
    pub traces_enabled: bool,
    pub poll_interval_ms: u64,
    pub geo_local_only: bool,
    pub show_low_confidence: bool,
    pub confidence_min: f64,
    pub globe_density: String,
    pub identify_domains: bool,
    pub history_enabled: bool,
    pub enhanced_monitoring: bool,
    /// Legacy compatibility flag. Monitoring now starts immediately.
    pub privacy_accepted: bool,
}

impl Default for SettingsDto {
    fn default() -> Self {
        Self {
            settings_version: 3,
            external_only: true,
            include_udp: true,
            traces_enabled: true,
            poll_interval_ms: 1000,
            geo_local_only: false,
            show_low_confidence: true,
            confidence_min: 0.45,
            globe_density: "all".into(),
            identify_domains: true,
            history_enabled: false,
            enhanced_monitoring: false,
            privacy_accepted: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathChangedEvent {
    pub app: String,
    pub host: String,
    pub ip: String,
    pub summary: String,
}

pub struct SnapshotContext<'a> {
    pub traces: &'a TraceEngine,
    pub geo: &'a GeoCache,
    pub path_geo: &'a PathGeoCache,
    pub asn: &'a AsnDb,
    pub settings: &'a SettingsDto,
    pub path_changed: &'a std::collections::HashSet<String>,
    pub traffic_status: &'a NativeTrafficStatus,
    pub collection_status: &'a CollectionStatus,
    pub destination_naming: &'a DestinationNamingStatus,
    pub udp_status: &'a UdpCollectionStatus,
    pub network_origin: &'a NetworkOriginView,
}

pub fn build_snapshot(state: &AppState, context: &SnapshotContext<'_>) -> SnapshotDto {
    let ts = context.traces.stats();
    let apps: Vec<AppDto> = state
        .sorted_apps()
        .into_iter()
        .map(|app| app_to_dto(app, context))
        .collect();
    let attributed = state.attribution.direct + state.attribution.recovered;
    let total = attributed + state.attribution.unattributed;
    let unattributed = state.unattributed();
    let unattributed = if unattributed.destinations.is_empty() {
        None
    } else {
        Some(TrafficGroupDto {
            name: "Unattributed traffic".into(),
            current_connections: unattributed.current_connections,
            connections: unattributed.connection_hits(),
            destinations: unattributed
                .sorted_destinations()
                .into_iter()
                .map(|dest| dest_to_dto(unattributed, dest, context))
                .collect(),
        })
    };

    let network_origin = build_network_origin(context.network_origin, &apps, unattributed.as_ref());

    SnapshotDto {
        app_count: state.app_count(),
        dest_count: state.total_destinations(),
        live_connections: state.last_raw_connections,
        missing_pid: state.missing_pid_count,
        attribution: AttributionStatsDto {
            direct: state.attribution.direct,
            recovered: state.attribution.recovered,
            unattributed: state.attribution.unattributed,
            ambiguous: state.attribution.ambiguous,
            owner_gone: state.attribution.owner_gone,
            access_limited: state.attribution.access_limited,
            ratio: if total == 0 {
                1.0
            } else {
                attributed as f64 / total as f64
            },
        },
        unattributed,
        monitoring: monitoring_to_dto(context.traffic_status),
        collection: CollectionDto {
            mode: context.collection_status.mode.into(),
            source: context.collection_status.source.into(),
            captures_opens: context.collection_status.captures_opens,
            captures_closes: context.collection_status.captures_closes,
            dropped_events: context.collection_status.dropped_events,
            status: context.collection_status.status.into(),
            message: context.collection_status.message.clone(),
            udp_remote: context.collection_status.udp_remote,
            access_limited: context.collection_status.access_limited,
            poll_phase: context.collection_status.poll_phase.into(),
            effective_poll_interval_ms: context.collection_status.effective_poll_interval_ms,
            observed_opens: context.collection_status.observed_opens,
            observed_closes: context.collection_status.observed_closes,
            recovered_owners: context.collection_status.recovered_owners,
            unattributed_owner_gone: context.collection_status.unattributed_owner_gone,
            unattributed_ambiguous: context.collection_status.unattributed_ambiguous,
            unattributed_access_limited: context.collection_status.unattributed_access_limited,
        },
        destination_naming: DestinationNamingDto {
            enabled: context.settings.identify_domains,
            status: context.destination_naming.status.into(),
            sources: context
                .destination_naming
                .sources
                .iter()
                .map(|source| (*source).into())
                .collect(),
            message: context.destination_naming.message.clone(),
        },
        udp_monitoring: udp_monitoring_to_dto(context.settings.include_udp, context.udp_status),
        external_only: state.external_only,
        include_udp: state.include_udp,
        traces_enabled: context.traces.enabled(),
        trace_stats: TraceStatsDto {
            queued: ts.queued,
            running: ts.running,
            done: ts.done,
            failed: ts.failed,
        },
        apps,
        geo_backend: context
            .geo
            .backend_label(context.settings.geo_local_only, context.asn.loaded()),
        geo_mmdb: context.geo.mmdb_loaded(),
        geo_asn_mmdb: context.asn.loaded(),
        settings: context.settings.clone(),
        network_origin,
    }
}

fn build_network_origin(
    origin: &NetworkOriginView,
    apps: &[AppDto],
    unattributed: Option<&TrafficGroupDto>,
) -> NetworkOriginDto {
    let hosted = origin.hosted.as_ref().map(|exit| NetworkExitDto {
        ip: Some(exit.ip.clone()),
        city: exit.city.clone(),
        country: exit.country.clone(),
        lat: exit.lat,
        lon: exit.lon,
        asn: exit.asn,
        organization: exit.organization.clone(),
        source: "hosted-egress".into(),
        confidence: exit.confidence,
        age_seconds: exit.observed_at.elapsed().as_secs(),
    });
    let fallback = hosted
        .is_none()
        .then(|| trace_origin(apps, unattributed))
        .flatten();
    let exit = hosted.or(fallback);
    let status = if exit.is_some() {
        "ready"
    } else if origin.hosted_attempted {
        "unavailable"
    } else {
        "locating"
    };
    NetworkOriginDto {
        status: status.into(),
        exit,
        assessment: origin.assessment.into(),
        evidence: origin
            .evidence
            .iter()
            .map(|item| NetworkOriginEvidenceDto {
                kind: item.kind.into(),
                strength: item.strength.into(),
                label: item.label.clone(),
            })
            .collect(),
    }
}

#[derive(Clone, Copy)]
struct TraceOriginCandidate<'a> {
    hop: &'a HopDto,
    hits: u64,
}

fn trace_origin(apps: &[AppDto], unattributed: Option<&TrafficGroupDto>) -> Option<NetworkExitDto> {
    let mut candidates = Vec::new();
    for app in apps {
        collect_trace_origins(&app.destinations, &mut candidates);
    }
    if let Some(group) = unattributed {
        collect_trace_origins(&group.destinations, &mut candidates);
    }
    let winner = candidates.iter().copied().max_by(|left, right| {
        let left_score = cluster_score(*left, &candidates);
        let right_score = cluster_score(*right, &candidates);
        left_score
            .cmp(&right_score)
            .then_with(|| right.hop.ttl.cmp(&left.hop.ttl))
    })?;
    Some(NetworkExitDto {
        ip: winner.hop.addr.clone(),
        city: winner.hop.city.clone(),
        country: winner.hop.country.clone(),
        lat: winner.hop.lat,
        lon: winner.hop.lon,
        asn: winner.hop.asn,
        organization: winner.hop.org.clone(),
        source: "trace-fallback".into(),
        confidence: winner.hop.geo_confidence,
        age_seconds: 0,
    })
}

fn collect_trace_origins<'a>(
    destinations: &'a [DestDto],
    candidates: &mut Vec<TraceOriginCandidate<'a>>,
) {
    for destination in destinations {
        if let Some(hop) = destination.trace.hops.iter().find(|hop| {
            hop.addr.is_some()
                && hop.lat.is_some()
                && hop.lon.is_some()
                && hop.geo_note.as_deref() != Some("private/local")
        }) {
            candidates.push(TraceOriginCandidate {
                hop,
                hits: destination.hits,
            });
        }
    }
}

fn cluster_score(
    candidate: TraceOriginCandidate<'_>,
    all: &[TraceOriginCandidate<'_>],
) -> (usize, u64) {
    let Some((lat, lon)) = candidate.hop.lat.zip(candidate.hop.lon) else {
        return (0, 0);
    };
    let nearby: Vec<_> = all
        .iter()
        .filter(|other| {
            other
                .hop
                .lat
                .zip(other.hop.lon)
                .is_some_and(|(other_lat, other_lon)| {
                    haversine_km(lat, lon, other_lat, other_lon) <= 150.0
                })
        })
        .collect();
    (nearby.len(), nearby.iter().map(|item| item.hits).sum())
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let radius = 6371.0;
    let radians = std::f64::consts::PI / 180.0;
    let value = ((lat2 - lat1) * radians / 2.0).sin().powi(2)
        + (lat1 * radians).cos()
            * (lat2 * radians).cos()
            * ((lon2 - lon1) * radians / 2.0).sin().powi(2);
    2.0 * radius * value.sqrt().asin()
}

fn udp_monitoring_to_dto(enabled: bool, status: &UdpCollectionStatus) -> UdpMonitoringDto {
    let (state, message) = match status {
        UdpCollectionStatus::Disabled => {
            ("disabled", "Connected UDP collection is disabled".into())
        }
        UdpCollectionStatus::Ready => ("ready", "Connected UDP peers are being collected".into()),
        UdpCollectionStatus::Degraded(message) => ("degraded", message.clone()),
        UdpCollectionStatus::Unavailable(message) => ("unavailable", message.clone()),
    };
    UdpMonitoringDto {
        enabled,
        coverage: "connected".into(),
        status: state.into(),
        message,
    }
}

fn app_to_dto(app: &AppEntry, context: &SnapshotContext<'_>) -> AppDto {
    let destinations: Vec<DestDto> = app
        .sorted_destinations()
        .into_iter()
        .map(|d| dest_to_dto(app, d, context))
        .collect();

    let name = display_name_for(app);
    let id = app.id.clone();

    AppDto {
        id,
        name,
        path: app.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
        pids: app.pids.iter().copied().collect(),
        processes: app
            .processes
            .values()
            .map(|process| ProcessDto {
                id: process.id.clone(),
                pid: process.pid,
                start_time: process.start_time,
                name: process.name.clone(),
                path: process
                    .path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                parent_pid: process.parent_pid,
                is_app_root: process.is_app_root,
            })
            .collect(),
        dest_count: app.destinations.len(),
        hits: app.connection_hits(),
        hits_per_sec: app.hits_per_sec,
        activity: app
            .traffic
            .map(|traffic| traffic.total_bytes_per_sec())
            .unwrap_or(app.hits_per_sec),
        current_connections: app.current_connections,
        new_connections_per_sec: app.hits_per_sec,
        traffic: app.traffic.map(|traffic| TrafficRateDto {
            rx_bytes_per_sec: traffic.rx_bytes_per_sec,
            tx_bytes_per_sec: traffic.tx_bytes_per_sec,
            total_bytes_per_sec: traffic.total_bytes_per_sec(),
            sample_window_ms: traffic.sample_window_ms,
            source: if cfg!(target_os = "linux") {
                "linux-sock-diag".into()
            } else {
                "native".into()
            },
        }),
        destinations,
    }
}

fn monitoring_to_dto(status: &NativeTrafficStatus) -> MonitoringDto {
    match status {
        NativeTrafficStatus::Disabled => MonitoringDto {
            mode: "portable".into(),
            status: "ready".into(),
            message: "Portable socket attribution".into(),
        },
        NativeTrafficStatus::Available => MonitoringDto {
            mode: "native".into(),
            status: "ready".into(),
            message: if cfg!(target_os = "linux") {
                "Linux TCP socket byte counters".into()
            } else {
                "Native byte counters".into()
            },
        },
        NativeTrafficStatus::Unavailable(message) => MonitoringDto {
            mode: "portable".into(),
            status: "unavailable".into(),
            message: message.clone(),
        },
    }
}

fn dest_to_dto(app: &AppEntry, d: &DestStats, context: &SnapshotContext<'_>) -> DestDto {
    let status = context.traces.get(d.remote.ip());
    let ip = d.remote.ip();
    let asn_info = context.asn.lookup(ip);
    let display_host = display_host_for(d);
    let change_key = format!("{}|{}", app.id, ip);
    DestDto {
        host: d.display_host(),
        display_host,
        ip: ip.to_string(),
        port: d.remote.port(),
        protocol: d.protocol.as_str().to_string(),
        hits: d.hit_count,
        last_seen_secs: d.last_seen.elapsed().as_secs(),
        sni: d.sni.clone(),
        domain: d.domain.as_ref().map(|name| name.value.clone()),
        domain_source: d
            .domain
            .as_ref()
            .map(|name| name.source.as_str().to_string())
            .or_else(|| d.hostname.as_ref().map(|_| "reverse-dns".into()))
            .unwrap_or_else(|| "ip".into()),
        domain_confidence: d
            .domain
            .as_ref()
            .map(|name| name.confidence.as_str().to_string())
            .or_else(|| d.hostname.as_ref().map(|_| "low".into()))
            .unwrap_or_else(|| "none".into()),
        domain_alternatives_count: d
            .domain
            .as_ref()
            .map(|name| name.alternatives_count)
            .unwrap_or(0),
        process_ids: d.process_ids.iter().cloned().collect(),
        asn: asn_info.as_ref().map(|a| a.asn),
        org: asn_info.map(|a| a.org),
        path_changed: context.path_changed.contains(&change_key),
        trace: trace_to_dto(
            status,
            context.geo,
            context.path_geo,
            context.asn,
            context.settings,
        ),
    }
}

fn display_host_for(d: &DestStats) -> String {
    if let Some(domain) = &d.domain {
        if !domain.value.is_empty() {
            return domain.value.clone();
        }
    }
    if let Some(h) = &d.hostname {
        if !h.is_empty() {
            return h.clone();
        }
    }
    d.remote.ip().to_string()
}

fn trace_to_dto(
    status: TraceStatus,
    geo: &GeoCache,
    path_geo: &PathGeoCache,
    asn: &AsnDb,
    settings: &SettingsDto,
) -> TraceDto {
    match status {
        TraceStatus::Idle => TraceDto {
            status: "idle".into(),
            label: "·".into(),
            hops: vec![],
            error: None,
            reached_target: false,
            target_rtt_ms: None,
        },
        TraceStatus::Queued => TraceDto {
            status: "queued".into(),
            label: "queued".into(),
            hops: vec![],
            error: None,
            reached_target: false,
            target_rtt_ms: None,
        },
        TraceStatus::Running => TraceDto {
            status: "running".into(),
            label: "tracing…".into(),
            hops: vec![],
            error: None,
            reached_target: false,
            target_rtt_ms: None,
        },
        TraceStatus::Failed { message, .. } => TraceDto {
            status: "failed".into(),
            label: format!("fail:{message}"),
            hops: vec![],
            error: Some(message),
            reached_target: false,
            target_rtt_ms: None,
        },
        TraceStatus::Done(r) => {
            let reached_target = r.reached_target();
            let target_rtt_ms = r.target_rtt_ms();
            let label = match (reached_target, target_rtt_ms, r.final_rtt_ms()) {
                (true, Some(ms), _) => format!("hops {}  {:.0}ms", r.hop_count(), ms),
                (true, None, _) => format!("hops {}  target reached", r.hop_count()),
                (false, _, Some(ms)) => format!("partial  last reply {:.0}ms", ms),
                (false, _, None) => "partial trace".into(),
            };
            let geo_hops = path_geo.get_or_compute(&r.hops, geo);
            let hops = geo_hops
                .into_iter()
                .filter_map(|h| geo_hop_to_dto(h, asn, settings))
                .collect();
            TraceDto {
                status: if r.hops.is_empty() {
                    "failed".into()
                } else {
                    "done".into()
                },
                label,
                hops,
                error: r.error.clone(),
                reached_target,
                target_rtt_ms,
            }
        }
    }
}

fn geo_hop_to_dto(h: GeoHop, asn: &AsnDb, settings: &SettingsDto) -> Option<HopDto> {
    // Confidence filter for plotted hops: still include hop without coords if unmapped
    if let Some(ref g) = h.geo {
        if !settings.show_low_confidence && g.confidence < settings.confidence_min {
            // strip geo but keep hop for list
            let ip = h.addr.as_ref().and_then(|a| a.parse().ok());
            let a = ip.and_then(|ip| asn.lookup(ip));
            return Some(HopDto {
                ttl: h.ttl,
                addr: h.addr,
                rtt_ms: h.rtt_ms,
                hostname: h.hostname,
                lat: None,
                lon: None,
                city: None,
                country: None,
                geo_source: Some(g.source.clone()),
                geo_confidence: Some(g.confidence),
                geo_note: Some("below confidence threshold".into()),
                asn: a.as_ref().map(|x| x.asn),
                org: a.map(|x| x.org),
            });
        }
    }

    let ip = h.addr.as_ref().and_then(|a| a.parse().ok());
    let a = ip.and_then(|ip| asn.lookup(ip));
    Some(HopDto {
        ttl: h.ttl,
        addr: h.addr,
        rtt_ms: h.rtt_ms,
        hostname: h.hostname,
        lat: h.geo.as_ref().map(|g| g.lat),
        lon: h.geo.as_ref().map(|g| g.lon),
        city: h.geo.as_ref().map(|g| g.city.clone()),
        country: h.geo.as_ref().map(|g| g.country.clone()),
        geo_source: h.geo.as_ref().map(|g| g.source.clone()),
        geo_confidence: h.geo.as_ref().map(|g| g.confidence),
        geo_note: h.note,
        asn: a.as_ref().map(|x| x.asn),
        org: a.map(|x| x.org),
    })
}

#[allow(dead_code)]
pub fn format_age(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 2 {
        "now".into()
    } else if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m", secs / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_deserialize_partial_json() {
        // Frontend may omit newer fields; serde default fills them.
        let raw = r#"{"externalOnly":false,"includeUdp":true,"tracesEnabled":true,"pollIntervalMs":2000}"#;
        let s: SettingsDto = serde_json::from_str(raw).expect("partial settings");
        assert!(!s.external_only);
        assert!(s.include_udp);
        assert_eq!(s.poll_interval_ms, 2000);
        assert!(s.privacy_accepted);
        assert_eq!(s.globe_density, "all");
        assert!(s.identify_domains);
    }

    #[test]
    fn settings_roundtrip_privacy() {
        let s = SettingsDto {
            privacy_accepted: true,
            ..SettingsDto::default()
        };
        let raw = serde_json::to_string(&s).unwrap();
        let back: SettingsDto = serde_json::from_str(&raw).unwrap();
        assert!(back.privacy_accepted);
    }

    #[test]
    fn trace_accuracy_fields_use_frontend_names() {
        let trace = TraceDto {
            status: "done".into(),
            label: "hops 4  12ms".into(),
            hops: vec![],
            error: None,
            reached_target: true,
            target_rtt_ms: Some(12.0),
        };
        let value = serde_json::to_value(trace).unwrap();
        assert_eq!(value["reachedTarget"], true);
        assert_eq!(value["targetRttMs"], 12.0);
        assert!(value.get("reached_target").is_none());
    }

    #[test]
    fn network_origin_fields_use_frontend_names() {
        let origin = NetworkOriginDto {
            status: "ready".into(),
            exit: Some(NetworkExitDto {
                ip: Some("1.1.1.1".into()),
                city: Some("Sydney".into()),
                country: Some("AU".into()),
                lat: Some(-33.86),
                lon: Some(151.2),
                asn: Some(13335),
                organization: Some("Cloudflare".into()),
                source: "hosted-egress".into(),
                confidence: Some(0.74),
                age_seconds: 3,
            }),
            assessment: "no_evidence".into(),
            evidence: Vec::new(),
        };
        let value = serde_json::to_value(origin).unwrap();
        assert_eq!(value["exit"]["ageSeconds"], 3);
        assert!(value["exit"].get("age_seconds").is_none());
    }

    #[test]
    fn trace_origin_cluster_count_wins_before_historical_activity() {
        fn hop(ttl: u8, lat: f64, lon: f64) -> HopDto {
            HopDto {
                ttl,
                addr: Some(format!("192.0.2.{ttl}")),
                rtt_ms: Some(10.0),
                hostname: None,
                lat: Some(lat),
                lon: Some(lon),
                city: None,
                country: None,
                geo_source: Some("mmdb".into()),
                geo_confidence: Some(0.8),
                geo_note: None,
                asn: None,
                org: None,
            }
        }
        let nearby_a = hop(2, 40.71, -74.00);
        let nearby_b = hop(3, 40.75, -73.95);
        let far_busy = hop(4, 51.50, -0.12);
        let candidates = vec![
            TraceOriginCandidate {
                hop: &nearby_a,
                hits: 1,
            },
            TraceOriginCandidate {
                hop: &nearby_b,
                hits: 2,
            },
            TraceOriginCandidate {
                hop: &far_busy,
                hits: 10_000,
            },
        ];
        assert_eq!(cluster_score(candidates[0], &candidates), (2, 3));
        assert_eq!(cluster_score(candidates[2], &candidates), (1, 10_000));
    }
}
