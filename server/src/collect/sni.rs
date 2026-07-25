//! Best-effort SNI cache for data supplied by unprivileged external hooks.

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
