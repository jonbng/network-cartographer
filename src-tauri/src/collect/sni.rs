//! Best-effort SNI cache. Full passive capture is optional / privilege-heavy;
//! this module provides the store that can be fed by future pcap or external hooks.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Entry {
    sni: String,
    at: Instant,
}

pub struct SniCache {
    map: Mutex<HashMap<IpAddr, Entry>>,
    ttl: Duration,
}

impl SniCache {
    pub fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(600),
        }
    }

    pub fn insert(&self, ip: IpAddr, sni: String) {
        if let Ok(mut m) = self.map.lock() {
            m.insert(ip, Entry { sni, at: Instant::now() });
        }
    }

    pub fn get(&self, ip: IpAddr) -> Option<String> {
        let mut m = self.map.lock().ok()?;
        let e = m.get(&ip)?;
        if e.at.elapsed() > self.ttl {
            m.remove(&ip);
            return None;
        }
        Some(e.sni.clone())
    }

    /// Apply known SNI onto destinations that match remote IPs.
    pub fn apply_to_state(&self, state: &mut crate::model::AppState) {
        let Ok(m) = self.map.lock() else {
            return;
        };
        for (ip, e) in m.iter() {
            if e.at.elapsed() > self.ttl {
                continue;
            }
            state.apply_sni(*ip, &e.sni);
        }
    }
}

impl Default for SniCache {
    fn default() -> Self {
        Self::new()
    }
}
