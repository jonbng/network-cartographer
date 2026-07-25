//! GeoIP + reverse-DNS cache with multi-source consensus and batch lookups.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::model::is_local_or_private;

use super::rdns;

#[derive(Debug, Clone)]
pub struct GeoHint {
    pub city: String,
    pub country: String,
    pub lat: f64,
    pub lon: f64,
    pub source: String,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct IpMeta {
    pub hostname: Option<String>,
    pub hints: Vec<GeoHint>,
    pub ready: bool,
    pub at: Instant,
}

#[derive(Debug, Deserialize)]
struct HostedGeoResponse {
    results: Vec<HostedGeoResult>,
}

#[derive(Debug, Deserialize)]
struct HostedGeoResult {
    ip: String,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    latitude: Option<f64>,
    #[serde(default)]
    longitude: Option<f64>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    confidence: Option<String>,
}

pub struct GeoCache {
    map: Mutex<HashMap<IpAddr, IpMeta>>,
    ttl: Duration,
    /// Optional MaxMind GeoLite2-City.mmdb (kept open for fast lookups).
    mmdb: Option<maxminddb::Reader<Vec<u8>>>,
}

impl GeoCache {
    pub fn new() -> Self {
        let mmdb_path = find_mmdb();
        let mmdb = mmdb_path
            .as_ref()
            .and_then(|p| match maxminddb::Reader::open_readfile(p) {
                Ok(r) => {
                    eprintln!("[geo] loaded MaxMind DB: {}", p.display());
                    Some(r)
                }
                Err(e) => {
                    eprintln!("[geo] failed to open {}: {e}", p.display());
                    None
                }
            });
        if mmdb.is_none() {
            eprintln!(
                "[geo] no local GeoLite2-City.mmdb — hosted geo via mapmy.network \
                 (optional offline DB: project root or data/GeoLite2-City.mmdb)"
            );
        }
        Self {
            map: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(6 * 3600),
            mmdb,
        }
    }

    /// Status string for UI, e.g. "mmdb+hosted" or "hosted".
    pub fn backend_label(&self, local_only: bool, asn: bool) -> String {
        let mut parts = Vec::new();
        if self.mmdb.is_some() {
            parts.push("mmdb");
        }
        if asn {
            parts.push("asn");
        }
        if !local_only {
            parts.push("hosted");
        }
        if parts.is_empty() {
            "rdns".into()
        } else {
            parts.join("+")
        }
    }

    pub fn mmdb_loaded(&self) -> bool {
        self.mmdb.is_some()
    }

    pub fn get(&self, ip: IpAddr) -> Option<IpMeta> {
        self.map.lock().ok().and_then(|m| m.get(&ip).cloned())
    }

    pub fn needs_resolve(&self, ip: IpAddr) -> bool {
        if is_local_or_private(ip) || !is_public_global(ip) {
            return false;
        }
        match self.map.lock().ok().and_then(|m| m.get(&ip).cloned()) {
            None => true,
            Some(m) => !m.ready || m.at.elapsed() >= self.ttl,
        }
    }

    /// Resolve many IPs: rDNS + mmdb, optional online sources.
    pub fn resolve_batch(&self, ips: &[IpAddr], local_only: bool) {
        let mut todo: Vec<IpAddr> = Vec::new();
        for &ip in ips {
            if !is_public_global(ip) || is_local_or_private(ip) {
                self.insert_ready(ip, None, vec![]);
                continue;
            }
            if self.needs_resolve(ip) {
                todo.push(ip);
            }
        }
        if todo.is_empty() {
            return;
        }

        // Stage 1: rDNS + optional local mmdb (no network rate limits)
        let mut partial: HashMap<IpAddr, (Option<String>, Vec<GeoHint>)> = HashMap::new();
        for &ip in &todo {
            let mut hints = Vec::new();
            let mut hostname = None;

            if let Ok(name) = dns_lookup::lookup_addr(&ip) {
                if !name.is_empty() && name != ip.to_string() {
                    hostname = Some(name.clone());
                    if let Some(h) = rdns::parse_hostname(&name) {
                        hints.push(h);
                    }
                }
            }

            if let Some(h) = self.lookup_mmdb(ip) {
                hints.push(h);
            }

            partial.insert(ip, (hostname, hints));
        }

        if !local_only {
            // Stage 2: Map My Network hosted geo (public hop IPs only)
            let need_hosted: Vec<IpAddr> = todo
                .iter()
                .copied()
                .filter(|ip| {
                    partial
                        .get(ip)
                        .map(|(_, h)| !h.iter().any(|x| x.source == "mmdb"))
                        .unwrap_or(true)
                })
                .collect();
            for chunk in need_hosted.chunks(40) {
                if let Some(batch) = fetch_hosted_geo(chunk) {
                    for (ip, hint) in batch {
                        if let Some((_, hints)) = partial.get_mut(&ip) {
                            hints.push(hint);
                        }
                    }
                }
            }
        }

        for (ip, (hostname, mut hints)) in partial {
            hints.retain(is_usable_city_hint);
            boost_consensus(&mut hints);
            self.insert_ready(ip, hostname, hints);
        }
    }

    fn insert_ready(&self, ip: IpAddr, hostname: Option<String>, hints: Vec<GeoHint>) {
        if let Ok(mut map) = self.map.lock() {
            map.insert(
                ip,
                IpMeta {
                    hostname,
                    hints,
                    ready: true,
                    at: Instant::now(),
                },
            );
            if map.len() > 12_000 {
                let ttl = self.ttl;
                map.retain(|_, m| m.at.elapsed() < ttl);
            }
        }
    }

    fn lookup_mmdb(&self, ip: IpAddr) -> Option<GeoHint> {
        let reader = self.mmdb.as_ref()?;
        let rec: maxminddb::geoip2::City = reader.lookup(ip).ok()?;
        let loc = rec.location.as_ref()?;
        let lat = loc.latitude?;
        let lon = loc.longitude?;
        if lat == 0.0 && lon == 0.0 {
            return None;
        }
        // No city name → country-level only (often "center of US") — skip
        let city = rec
            .city
            .as_ref()
            .and_then(|c| c.names.as_ref())
            .and_then(|n| n.get("en").copied())
            .filter(|s| !s.is_empty())?
            .to_string();
        let country = rec
            .country
            .as_ref()
            .and_then(|c| c.iso_code)
            .unwrap_or("")
            .to_string();
        let hint = GeoHint {
            city,
            country,
            lat,
            lon,
            source: "mmdb".into(),
            confidence: 0.78,
        };
        is_usable_city_hint(&hint).then_some(hint)
    }
}

/// Drop country-default pins (classic MaxMind "middle of Kansas" + "Unknown").
pub fn is_usable_city_hint(h: &GeoHint) -> bool {
    let city = h.city.trim();
    if city.is_empty()
        || city.eq_ignore_ascii_case("unknown")
        || city.eq_ignore_ascii_case("n/a")
        || city.eq_ignore_ascii_case("null")
        || city.eq_ignore_ascii_case("none")
    {
        return false;
    }
    if is_default_geo_coordinate(h.lat, h.lon) {
        return false;
    }
    true
}

/// Well-known GeoIP country/continent fallbacks (not real cities).
fn is_default_geo_coordinate(lat: f64, lon: f64) -> bool {
    // (lat, lon, radius_km) — MaxMind & others park "US unknown" near Kansas
    const DEFAULTS: &[(f64, f64, f64)] = &[
        (37.751, -97.822, 40.0),   // MaxMind US default
        (39.76, -98.5, 80.0),      // geographic center US-ish
        (38.0, -97.0, 50.0),       // common round default
        (39.8283, -98.5795, 40.0), // geographic center of contiguous US
        (37.0902, -95.7129, 60.0), // "center of US" used by some APIs
        (54.0, -2.0, 50.0),        // UK country default-ish
        (51.5, 10.5, 80.0),        // Germany center-ish
        (56.0, 10.0, 80.0),        // Europe blob
        (35.0, 105.0, 100.0),      // China center-ish
        (20.0, 77.0, 100.0),       // India center-ish
        (-25.0, 135.0, 100.0),     // Australia center-ish
        (0.0, 0.0, 30.0),          // null island
    ];
    for &(dlat, dlon, radius) in DEFAULTS {
        if haversine_km(lat, lon, dlat, dlon) <= radius {
            return true;
        }
    }
    false
}

fn boost_consensus(hints: &mut [GeoHint]) {
    if hints.len() < 2 {
        return;
    }
    // If two sources within ~50km, boost both
    for i in 0..hints.len() {
        for j in (i + 1)..hints.len() {
            let d = haversine_km(hints[i].lat, hints[i].lon, hints[j].lat, hints[j].lon);
            if d < 50.0 {
                hints[i].confidence = (hints[i].confidence + 0.12).min(0.95);
                hints[j].confidence = (hints[j].confidence + 0.12).min(0.95);
            } else if d > 500.0 {
                // Discord: slightly demote hosted vs rDNS/mmdb when far apart
                if hints[i].source == "hosted" || hints[i].source == "geolite" {
                    hints[i].confidence = (hints[i].confidence - 0.08).max(0.2);
                }
                if hints[j].source == "hosted" || hints[j].source == "geolite" {
                    hints[j].confidence = (hints[j].confidence - 0.08).max(0.2);
                }
            }
        }
    }
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0;
    let p = std::f64::consts::PI / 180.0;
    let a = ((lat2 - lat1) * p / 2.0).sin().powi(2)
        + (lat1 * p).cos() * (lat2 * p).cos() * ((lon2 - lon1) * p / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn find_mmdb() -> Option<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();

    // Explicit override
    if let Ok(p) = std::env::var("NETWORK_CARTOGRAPHER_MMDB") {
        paths.push(PathBuf::from(p));
    }

    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join("GeoLite2-City.mmdb"));
        paths.push(cwd.join("data/GeoLite2-City.mmdb"));
        paths.push(cwd.join("server/data/GeoLite2-City.mmdb"));
        // Cargo commands are often run with cwd = server
        paths.push(cwd.join("../GeoLite2-City.mmdb"));
        paths.push(cwd.join("../data/GeoLite2-City.mmdb"));
        if let Some(parent) = cwd.parent() {
            paths.push(parent.join("GeoLite2-City.mmdb"));
            paths.push(parent.join("data/GeoLite2-City.mmdb"));
        }
    }

    // Next to the binary
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join("GeoLite2-City.mmdb"));
            paths.push(dir.join("data/GeoLite2-City.mmdb"));
            // cargo target/debug → repo root ../../
            paths.push(dir.join("../../../GeoLite2-City.mmdb"));
            paths.push(dir.join("../../../../GeoLite2-City.mmdb"));
        }
    }

    // User / system locations (platform-specific)
    #[cfg(unix)]
    {
        if let Ok(home) = std::env::var("HOME") {
            let home = PathBuf::from(home);
            paths.push(home.join(".local/share/GeoIP/GeoLite2-City.mmdb"));
            paths.push(home.join("GeoLite2-City.mmdb"));
            #[cfg(target_os = "macos")]
            {
                paths.push(home.join("Library/Application Support/GeoIP/GeoLite2-City.mmdb"));
                paths.push(
                    home.join(
                        "Library/Application Support/network-cartographer/GeoLite2-City.mmdb",
                    ),
                );
            }
        }
        paths.push(PathBuf::from("/usr/share/GeoIP/GeoLite2-City.mmdb"));
        paths.push(PathBuf::from("/var/lib/GeoIP/GeoLite2-City.mmdb"));
    }

    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let local = PathBuf::from(local);
            paths.push(local.join("GeoIP").join("GeoLite2-City.mmdb"));
            paths.push(
                local
                    .join("network-cartographer")
                    .join("GeoLite2-City.mmdb"),
            );
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            paths.push(PathBuf::from(profile).join("GeoLite2-City.mmdb"));
        }
    }

    for p in paths {
        if let Ok(canon) = p.canonicalize() {
            if canon.is_file() {
                return Some(canon);
            }
        } else if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn is_public_global(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_unspecified()
                || v4.is_loopback()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_link_local()
                || v4.is_private()
                || v4.is_documentation())
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

const USER_AGENT: &str = concat!(
    "netcart/",
    env!("CARGO_PKG_VERSION"),
    " (+https://mapmy.network)"
);

const DEFAULT_HOSTED_GEO_URL: &str = "https://mapmy.network/api/v1/geo";

fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(6))
        .user_agent(USER_AGENT)
        .build()
}

fn hosted_geo_url() -> String {
    std::env::var("NETWORK_CARTOGRAPHER_GEO_URL")
        .or_else(|_| std::env::var("NETCART_GEO_URL"))
        .unwrap_or_else(|_| DEFAULT_HOSTED_GEO_URL.to_string())
}

fn confidence_from_label(label: Option<&str>) -> f64 {
    match label.map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("high") => 0.82,
        Some("medium") => 0.74,
        Some("low") => 0.55,
        _ => 0.7,
    }
}

fn fetch_hosted_geo(ips: &[IpAddr]) -> Option<Vec<(IpAddr, GeoHint)>> {
    if ips.is_empty() {
        return None;
    }
    let url = hosted_geo_url();
    let body = serde_json::json!({
        "ips": ips.iter().map(|ip| ip.to_string()).collect::<Vec<_>>(),
    });

    let resp = match http_agent()
        .post(&url)
        .set("Content-Type", "application/json")
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/json")
        .send_json(body)
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[geo] hosted lookup failed: {e}");
            return None;
        }
    };

    if resp.status() >= 400 {
        eprintln!("[geo] hosted lookup HTTP {}", resp.status());
        return None;
    }

    let parsed: HostedGeoResponse = match resp.into_json() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[geo] hosted lookup decode failed: {e}");
            return None;
        }
    };

    let mut out = Vec::new();
    for row in parsed.results {
        let ip: IpAddr = match row.ip.parse() {
            Ok(ip) => ip,
            Err(_) => continue,
        };
        let lat = match row.latitude {
            Some(v) => v,
            None => continue,
        };
        let lon = match row.longitude {
            Some(v) => v,
            None => continue,
        };
        if lat == 0.0 && lon == 0.0 {
            continue;
        }
        let city = match row.city.filter(|c| !c.is_empty()) {
            Some(c) => c,
            None => continue,
        };
        let source = row
            .source
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "hosted".into());
        let hint = GeoHint {
            city,
            country: row.country.unwrap_or_default(),
            lat,
            lon,
            source,
            confidence: confidence_from_label(row.confidence.as_deref()),
        };
        if !is_usable_city_hint(&hint) {
            continue;
        }
        out.push((ip, hint));
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

impl Default for GeoCache {
    fn default() -> Self {
        Self::new()
    }
}
