use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::geo::{AsnDb, GeoCache, GeoHop, PathGeoCache};
use crate::model::{display_name_for, AppEntry, AppState, DestStats};
use crate::privileges;
use crate::trace::{TraceEngine, TraceStatus};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDto {
    pub apps: Vec<AppDto>,
    pub app_count: usize,
    pub dest_count: usize,
    pub live_connections: usize,
    pub missing_pid: usize,
    pub external_only: bool,
    pub include_udp: bool,
    pub traces_enabled: bool,
    pub trace_stats: TraceStatsDto,
    pub geo_backend: String,
    pub geo_mmdb: bool,
    pub geo_asn_mmdb: bool,
    pub elevated: bool,
    pub elevation_hint: Option<String>,
    pub settings: SettingsDto,
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
    pub dest_count: usize,
    pub hits: u64,
    pub hits_per_sec: f64,
    pub activity: f64,
    pub destinations: Vec<DestDto>,
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
    pub external_only: bool,
    pub include_udp: bool,
    pub traces_enabled: bool,
    pub poll_interval_ms: u64,
    pub geo_local_only: bool,
    pub show_low_confidence: bool,
    pub confidence_min: f64,
    pub globe_density: String,
    pub capture_sni: bool,
    pub history_enabled: bool,
    /// User accepted first-run privacy notice (GeoIP / local monitoring).
    pub privacy_accepted: bool,
}

impl Default for SettingsDto {
    fn default() -> Self {
        Self {
            external_only: true,
            include_udp: false,
            traces_enabled: true,
            poll_interval_ms: 1000,
            geo_local_only: false,
            show_low_confidence: true,
            confidence_min: 0.45,
            globe_density: "all".into(),
            capture_sni: false,
            history_enabled: false,
            privacy_accepted: false,
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

struct DtoContext<'a> {
    traces: &'a TraceEngine,
    geo: &'a GeoCache,
    path_geo: &'a PathGeoCache,
    asn: &'a AsnDb,
    settings: &'a SettingsDto,
    path_changed: &'a std::collections::HashSet<String>,
}

pub fn build_snapshot(
    state: &AppState,
    traces: &TraceEngine,
    geo: &GeoCache,
    path_geo: &PathGeoCache,
    asn: &AsnDb,
    settings: &SettingsDto,
    path_changed: &std::collections::HashSet<String>,
) -> SnapshotDto {
    let ts = traces.stats();
    let context = DtoContext {
        traces,
        geo,
        path_geo,
        asn,
        settings,
        path_changed,
    };
    let apps: Vec<AppDto> = state
        .sorted_apps()
        .into_iter()
        .map(|app| app_to_dto(app, &context))
        .collect();

    SnapshotDto {
        app_count: state.app_count(),
        dest_count: state.total_destinations(),
        live_connections: state.last_raw_connections,
        missing_pid: state.missing_pid_count,
        external_only: state.external_only,
        include_udp: state.include_udp,
        traces_enabled: traces.enabled(),
        trace_stats: TraceStatsDto {
            queued: ts.queued,
            running: ts.running,
            done: ts.done,
            failed: ts.failed,
        },
        apps,
        geo_backend: geo.backend_label(settings.geo_local_only, asn.loaded()),
        geo_mmdb: geo.mmdb_loaded(),
        geo_asn_mmdb: asn.loaded(),
        elevated: privileges::is_elevated(),
        elevation_hint: privileges::elevation_hint().map(|s| s.to_string()),
        settings: settings.clone(),
    }
}

fn app_to_dto(app: &AppEntry, context: &DtoContext<'_>) -> AppDto {
    let destinations: Vec<DestDto> = app
        .sorted_destinations()
        .into_iter()
        .map(|d| dest_to_dto(app, d, context))
        .collect();

    let name = display_name_for(app);
    let id = app
        .path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| app.name.clone());

    AppDto {
        id,
        name,
        path: app.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
        pids: app.pids.iter().copied().collect(),
        dest_count: app.destinations.len(),
        hits: app.connection_hits(),
        hits_per_sec: app.hits_per_sec,
        activity: app.hits_per_sec,
        destinations,
    }
}

fn dest_to_dto(
    app: &AppEntry,
    d: &DestStats,
    context: &DtoContext<'_>,
) -> DestDto {
    let status = context.traces.get(d.remote.ip());
    let ip = d.remote.ip();
    let asn_info = context.asn.lookup(ip);
    let display_host = display_host_for(d);
    let change_key = format!("{}|{}", display_name_for(app), ip);
    DestDto {
        host: d.display_host(),
        display_host,
        ip: ip.to_string(),
        port: d.remote.port(),
        protocol: d.protocol.as_str().to_string(),
        hits: d.hit_count,
        last_seen_secs: d.last_seen.elapsed().as_secs(),
        sni: d.sni.clone(),
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
    if let Some(sni) = &d.sni {
        if !sni.is_empty() {
            return sni.clone();
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
        },
        TraceStatus::Queued => TraceDto {
            status: "queued".into(),
            label: "queued".into(),
            hops: vec![],
            error: None,
        },
        TraceStatus::Running => TraceDto {
            status: "running".into(),
            label: "tracing…".into(),
            hops: vec![],
            error: None,
        },
        TraceStatus::Failed { message, .. } => TraceDto {
            status: "failed".into(),
            label: format!("fail:{message}"),
            hops: vec![],
            error: Some(message),
        },
        TraceStatus::Done(r) => {
            let label = match r.final_rtt_ms() {
                Some(ms) => format!("hops {}  {:.0}ms", r.hop_count(), ms),
                None => format!("hops {}", r.hop_count()),
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
        assert!(!s.privacy_accepted);
        assert_eq!(s.globe_density, "all");
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
}
