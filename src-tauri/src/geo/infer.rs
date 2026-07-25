//! Path-level geolocation with latency consistency checks.
//! Inspired by GeoTraceroute-style heuristics (simplified, real-time, no DB of paths).

use std::net::IpAddr;

use serde::Serialize;

use super::lookup::{is_usable_city_hint, GeoCache, GeoHint};
use crate::model::is_local_or_private;
use crate::trace::Hop;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoLocation {
    pub lat: f64,
    pub lon: f64,
    pub city: String,
    pub country: String,
    pub source: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoHop {
    pub ttl: u8,
    pub addr: Option<String>,
    pub rtt_ms: Option<f64>,
    pub hostname: Option<String>,
    pub geo: Option<GeoLocation>,
    pub note: Option<String>,
}

/// Build geolocated hops from cache only (non-blocking).
pub fn geolocate_path(hops: &[Hop], cache: &GeoCache) -> Vec<GeoHop> {
    let mut out: Vec<GeoHop> = Vec::with_capacity(hops.len());

    for hop in hops {
        let mut hostname = None;
        let mut candidates: Vec<GeoHint> = Vec::new();

        if let Some(ip) = hop.addr {
            if is_local_or_private(ip) {
                out.push(GeoHop {
                    ttl: hop.ttl,
                    addr: Some(ip.to_string()),
                    rtt_ms: hop.rtt_ms,
                    hostname: None,
                    geo: None,
                    note: Some("private/local".into()),
                });
                continue;
            }

            if let Some(meta) = cache.get(ip) {
                hostname = meta.hostname;
                // Never plot country-default / "Unknown" city pins
                candidates = meta
                    .hints
                    .into_iter()
                    .filter(is_usable_city_hint)
                    .collect();
            }
        }

        let chosen = pick_candidate(&candidates, hop.rtt_ms, out.last());
        let note = if chosen.is_none()
            && hop.addr.is_some()
            && !hop
                .addr
                .map(is_local_or_private)
                .unwrap_or(true)
        {
            // Had an IP but no trustworthy city-level geo
            Some("no city-level geo".into())
        } else {
            None
        };

        out.push(GeoHop {
            ttl: hop.ttl,
            addr: hop.addr.map(|a| a.to_string()),
            rtt_ms: hop.rtt_ms,
            hostname,
            geo: chosen.map(|h| GeoLocation {
                lat: h.lat,
                lon: h.lon,
                city: h.city,
                country: h.country,
                source: h.source,
                confidence: h.confidence,
            }),
            note,
        });
    }

    refine_path(&mut out);
    out
}

/// Collect hop IPs that still need geo resolution.
pub fn pending_ips(hops: &[Hop], cache: &GeoCache) -> Vec<IpAddr> {
    hops.iter()
        .filter_map(|h| h.addr)
        .filter(|ip| cache.needs_resolve(*ip))
        .collect()
}

fn pick_candidate(
    candidates: &[GeoHint],
    rtt_ms: Option<f64>,
    prev: Option<&GeoHop>,
) -> Option<GeoHint> {
    let candidates: Vec<&GeoHint> = candidates.iter().filter(|c| is_usable_city_hint(c)).collect();
    if candidates.is_empty() {
        return None;
    }

    let mut best: Option<(f64, GeoHint)> = None;
    for c in candidates {
        let mut score = c.confidence;

        if c.source.starts_with("rdns") {
            score += 0.15;
        }
        // Prefer named cities from rDNS/mmdb over generic API blobs
        if c.source == "mmdb" {
            score += 0.08;
        }

        if let (Some(rtt), Some(prev_hop)) = (rtt_ms, prev) {
            if let (Some(prev_geo), Some(prev_rtt)) = (&prev_hop.geo, prev_hop.rtt_ms) {
                let dist_km = haversine_km(prev_geo.lat, prev_geo.lon, c.lat, c.lon);
                let drtt = (rtt - prev_rtt).abs();
                let expected = dist_km * 0.01;
                if expected > 1.0 {
                    if drtt + 5.0 < expected * 0.35 {
                        score -= 0.25;
                    } else if drtt > expected * 3.0 + 40.0 {
                        score -= 0.08;
                    } else {
                        score += 0.05;
                    }
                }
                if same_metro_hint(prev_geo, c) && drtt > 25.0 && c.source == "geoip" {
                    score -= 0.2;
                }
                if dist_km > 3000.0 && rtt < 15.0 {
                    score -= 0.3;
                }
            }
        }

        match &best {
            None => best = Some((score, c.clone())),
            Some((s, _)) if score > *s => best = Some((score, c.clone())),
            _ => {}
        }
    }

    best.map(|(score, mut h)| {
        h.confidence = score.clamp(0.05, 0.98);
        h
    })
}

fn refine_path(hops: &mut [GeoHop]) {
    for i in 0..hops.len() {
        let prev_geo = if i > 0 {
            hops[i - 1].geo.clone()
        } else {
            None
        };
        let prev_rtt = if i > 0 { hops[i - 1].rtt_ms } else { None };
        let next_geo = if i + 1 < hops.len() {
            hops[i + 1].geo.clone()
        } else {
            None
        };
        let next_rtt = if i + 1 < hops.len() {
            hops[i + 1].rtt_ms
        } else {
            None
        };

        let Some(cur) = hops[i].geo.clone() else {
            continue;
        };
        let cur_rtt = hops[i].rtt_ms;

        if !cur.source.starts_with("geoip") || cur.confidence >= 0.7 {
            continue;
        }

        if let (Some(pg), Some(pr), Some(cr)) = (&prev_geo, prev_rtt, cur_rtt) {
            let d = haversine_km(pg.lat, pg.lon, cur.lat, cur.lon);
            let drtt = (cr - pr).max(0.0);
            let expected = d * 0.01;
            if expected > 15.0 && drtt < expected * 0.25 {
                hops[i].note = Some(format!(
                    "relocated: latency {:.0}ms too small for {:.0}km GeoIP jump",
                    drtt, d
                ));
                hops[i].geo = Some(GeoLocation {
                    lat: pg.lat,
                    lon: pg.lon,
                    city: pg.city.clone(),
                    country: pg.country.clone(),
                    source: "inferred-latency".into(),
                    confidence: 0.4,
                });
                continue;
            }
        }

        if let (Some(pg), Some(ng), Some(pr), Some(_cr), Some(nr)) =
            (&prev_geo, &next_geo, prev_rtt, cur_rtt, next_rtt)
        {
            if same_metro_geo(pg, ng) && !same_metro_geo(pg, &cur) {
                let d = haversine_km(pg.lat, pg.lon, cur.lat, cur.lon);
                let span = (nr - pr).abs();
                if d > 800.0 && span < 12.0 {
                    hops[i].note = Some("relocated: path oscillation".into());
                    hops[i].geo = Some(GeoLocation {
                        lat: pg.lat,
                        lon: pg.lon,
                        city: pg.city.clone(),
                        country: pg.country.clone(),
                        source: "inferred-path".into(),
                        confidence: 0.45,
                    });
                }
            }
        }
    }
}

fn same_metro_hint(a: &GeoLocation, b: &GeoHint) -> bool {
    haversine_km(a.lat, a.lon, b.lat, b.lon) < 80.0
        || (a.city.eq_ignore_ascii_case(&b.city) && !a.city.is_empty())
}

fn same_metro_geo(a: &GeoLocation, b: &GeoLocation) -> bool {
    haversine_km(a.lat, a.lon, b.lat, b.lon) < 80.0
        || (a.city.eq_ignore_ascii_case(&b.city) && !a.city.is_empty())
}

pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0;
    let p = std::f64::consts::PI / 180.0;
    let a = ((lat2 - lat1) * p / 2.0).sin().powi(2)
        + (lat1 * p).cos() * (lat2 * p).cos() * ((lon2 - lon1) * p / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}
