//! Cache fully geolocated traceroute paths so snapshots stay cheap.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::infer::{geolocate_path, GeoHop};
use super::lookup::GeoCache;
use crate::trace::Hop;

pub struct PathGeoCache {
    map: Mutex<HashMap<u64, (Vec<GeoHop>, Instant)>>,
    ttl: Duration,
}

impl PathGeoCache {
    pub fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(120),
        }
    }

    pub fn get_or_compute(&self, hops: &[Hop], geo: &GeoCache) -> Vec<GeoHop> {
        let key = path_fingerprint(hops, geo);
        if let Ok(map) = self.map.lock() {
            if let Some((cached, at)) = map.get(&key) {
                if at.elapsed() < self.ttl {
                    return cached.clone();
                }
            }
        }

        let computed = geolocate_path(hops, geo);
        if let Ok(mut map) = self.map.lock() {
            map.insert(key, (computed.clone(), Instant::now()));
            if map.len() > 2048 {
                let ttl = self.ttl;
                map.retain(|_, (_, t)| t.elapsed() < ttl);
            }
        }
        computed
    }

    pub fn clear(&self) {
        if let Ok(mut map) = self.map.lock() {
            map.clear();
        }
    }
}

impl Default for PathGeoCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Fingerprint hops + which IPs already have geo ready (so cache invalidates as geo fills).
fn path_fingerprint(hops: &[Hop], geo: &GeoCache) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for h in hops {
        h.ttl.hash(&mut hasher);
        match h.addr {
            Some(ip) => {
                ip.hash(&mut hasher);
                // Include whether geo is ready + coarse city so results update when geo arrives
                if let Some(meta) = geo.get(ip) {
                    meta.ready.hash(&mut hasher);
                    for hint in meta.hints.iter().take(2) {
                        // quantize lat/lon
                        ((hint.lat * 100.0) as i32).hash(&mut hasher);
                        ((hint.lon * 100.0) as i32).hash(&mut hasher);
                        hint.source.hash(&mut hasher);
                    }
                } else {
                    0u8.hash(&mut hasher);
                }
            }
            None => {
                0u8.hash(&mut hasher);
            }
        }
        // quantize rtt to 5ms buckets — avoid thrash
        let rtt_bucket = h.rtt_ms.map(|ms| (ms / 5.0) as i32).unwrap_or(-1);
        rtt_bucket.hash(&mut hasher);
    }
    hasher.finish()
}
