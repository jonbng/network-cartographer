//! Private GeoLite2 City + ASN lookup service for Map My Network.
//!
//! Intended to run on a VPS behind a bearer token. The public surface is
//! `https://mapmy.network/api/v1/geo`, which proxies here after validation.

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use maxminddb::Reader;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;

const MAX_BATCH: usize = 40;

#[derive(Clone)]
struct AppState {
    token: Arc<str>,
    dbs: Arc<RwLock<GeoDbs>>,
}

struct GeoDbs {
    city: Option<Reader<Vec<u8>>>,
    asn: Option<Reader<Vec<u8>>>,
    city_path: PathBuf,
    asn_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct LookupRequest {
    ips: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
struct GeoResult {
    ip: String,
    city: Option<String>,
    country: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    asn: Option<u32>,
    organization: Option<String>,
    source: String,
    confidence: Option<&'static str>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mapmy_geo_service=info,tower_http=info".into()),
        )
        .init();

    let token = std::env::var("GEO_SERVICE_TOKEN").unwrap_or_else(|_| {
        eprintln!("GEO_SERVICE_TOKEN is required");
        std::process::exit(1);
    });
    if token.len() < 16 {
        eprintln!("GEO_SERVICE_TOKEN must be at least 16 characters");
        std::process::exit(1);
    }

    let city_path = env_path("GEO_CITY_MMDB", "data/GeoLite2-City.mmdb");
    let asn_path = env_path("GEO_ASN_MMDB", "data/GeoLite2-ASN.mmdb");
    let listen = std::env::var("GEO_LISTEN").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let addr: SocketAddr = listen.parse().unwrap_or_else(|e| {
        eprintln!("invalid GEO_LISTEN {listen}: {e}");
        std::process::exit(1);
    });

    let dbs = GeoDbs::load(city_path, asn_path);
    if dbs.city.is_none() {
        eprintln!(
            "warning: city MMDB not loaded — lookups will return empty city fields \
             (set GEO_CITY_MMDB or place GeoLite2-City.mmdb under data/)"
        );
    }

    let state = AppState {
        token: Arc::from(token),
        dbs: Arc::new(RwLock::new(dbs)),
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/lookup", post(lookup))
        .route("/v1/reload", post(reload))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    tracing::info!("listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("serve");
}

fn env_path(key: &str, default: &str) -> PathBuf {
    std::env::var(key)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default))
}

impl GeoDbs {
    fn load(city_path: PathBuf, asn_path: PathBuf) -> Self {
        let city = open_mmdb(&city_path, "city");
        let asn = open_mmdb(&asn_path, "asn");
        Self {
            city,
            asn,
            city_path,
            asn_path,
        }
    }

    fn reload(&mut self) {
        self.city = open_mmdb(&self.city_path, "city");
        self.asn = open_mmdb(&self.asn_path, "asn");
    }

    fn lookup_one(&self, ip: IpAddr) -> GeoResult {
        let ip_s = ip.to_string();
        let mut result = GeoResult {
            ip: ip_s,
            city: None,
            country: None,
            latitude: None,
            longitude: None,
            asn: None,
            organization: None,
            source: "geolite".into(),
            confidence: None,
        };

        if let Some(reader) = self.city.as_ref() {
            if let Ok(rec) = reader.lookup::<maxminddb::geoip2::City>(ip) {
                let country = rec
                    .country
                    .as_ref()
                    .and_then(|c| c.iso_code)
                    .map(|s| s.to_string());
                let city = rec
                    .city
                    .as_ref()
                    .and_then(|c| c.names.as_ref())
                    .and_then(|n| n.get("en").copied())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                let (lat, lon) = rec
                    .location
                    .as_ref()
                    .map(|l| (l.latitude, l.longitude))
                    .unwrap_or((None, None));

                if let (Some(city), Some(lat), Some(lon)) = (city, lat, lon) {
                    if lat != 0.0
                        && lon != 0.0
                        && is_usable_city(&city, lat, lon)
                    {
                        result.city = Some(city);
                        result.country = country;
                        result.latitude = Some(lat);
                        result.longitude = Some(lon);
                        result.confidence = Some("medium");
                    } else {
                        result.country = country;
                    }
                } else {
                    result.country = country;
                }
            }
        }

        if let Some(reader) = self.asn.as_ref() {
            #[derive(Deserialize)]
            struct AsnRecord {
                autonomous_system_number: Option<u32>,
                autonomous_system_organization: Option<String>,
            }
            if let Ok(rec) = reader.lookup::<AsnRecord>(ip) {
                result.asn = rec.autonomous_system_number;
                result.organization = rec
                    .autonomous_system_organization
                    .filter(|s| !s.is_empty())
                    .or_else(|| result.asn.map(|n| format!("AS{n}")));
            }
        }

        result
    }
}

fn open_mmdb(path: &Path, label: &str) -> Option<Reader<Vec<u8>>> {
    match Reader::open_readfile(path) {
        Ok(r) => {
            tracing::info!("loaded {label} MMDB from {}", path.display());
            Some(r)
        }
        Err(e) => {
            tracing::warn!("failed to open {label} MMDB {}: {e}", path.display());
            None
        }
    }
}

fn authorize(headers: &HeaderMap, expected: &str) -> Result<(), StatusCode> {
    let Some(value) = headers.get(axum::http::header::AUTHORIZATION) else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let Ok(raw) = value.to_str() else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let Some(token) = raw.strip_prefix("Bearer ") else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    if token != expected {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(())
}

async fn healthz(State(state): State<AppState>) -> Json<serde_json::Value> {
    let dbs = state.dbs.read().await;
    Json(serde_json::json!({
        "ok": true,
        "city_mmdb": dbs.city.is_some(),
        "asn_mmdb": dbs.asn.is_some(),
    }))
}

async fn reload(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    authorize(&headers, &state.token)?;
    {
        let mut dbs = state.dbs.write().await;
        dbs.reload();
    }
    let dbs = state.dbs.read().await;
    Ok(Json(serde_json::json!({
        "reloaded": true,
        "city_mmdb": dbs.city.is_some(),
        "asn_mmdb": dbs.asn.is_some(),
    })))
}

async fn lookup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LookupRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    authorize(&headers, &state.token)?;

    if body.ips.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if body.ips.len() > MAX_BATCH {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    let mut ips: Vec<IpAddr> = Vec::with_capacity(body.ips.len());
    for raw in &body.ips {
        let trimmed = raw.trim();
        let Ok(ip) = trimmed.parse::<IpAddr>() else {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        };
        if !is_public_global(ip) {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        ips.push(ip);
    }
    ips.sort_unstable();
    ips.dedup();

    let dbs = state.dbs.read().await;
    let results: Vec<GeoResult> = ips.into_iter().map(|ip| dbs.lookup_one(ip)).collect();
    Ok(Json(serde_json::json!({ "results": results })))
}

fn is_public_global(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast())
        }
        IpAddr::V6(v6) => {
            !(v6.is_unspecified()
                || v6.is_loopback()
                || v6.is_multicast()
                || v6.is_unicast_link_local()
                || v6.is_unique_local())
        }
    }
}

fn is_usable_city(city: &str, lat: f64, lon: f64) -> bool {
    let city = city.trim();
    if city.is_empty()
        || city.eq_ignore_ascii_case("unknown")
        || city.eq_ignore_ascii_case("n/a")
        || city.eq_ignore_ascii_case("null")
        || city.eq_ignore_ascii_case("none")
    {
        return false;
    }
    !is_default_geo_coordinate(lat, lon)
}

fn is_default_geo_coordinate(lat: f64, lon: f64) -> bool {
    const DEFAULTS: &[(f64, f64, f64)] = &[
        (37.751, -97.822, 40.0),
        (39.76, -98.5, 80.0),
        (38.0, -97.0, 50.0),
        (39.8283, -98.5795, 40.0),
        (37.0902, -95.7129, 60.0),
        (54.0, -2.0, 50.0),
        (51.5, 10.5, 80.0),
        (56.0, 10.0, 80.0),
        (35.0, 105.0, 100.0),
        (20.0, 77.0, 100.0),
        (-25.0, 135.0, 100.0),
        (0.0, 0.0, 30.0),
    ];
    for &(dlat, dlon, radius) in DEFAULTS {
        if haversine_km(lat, lon, dlat, dlon) <= radius {
            return true;
        }
    }
    false
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0;
    let p = std::f64::consts::PI / 180.0;
    let a = ((lat2 - lat1) * p / 2.0).sin().powi(2)
        + (lat1 * p).cos() * (lat2 * p).cos() * ((lon2 - lon1) * p / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
