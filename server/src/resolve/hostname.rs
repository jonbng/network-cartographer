use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Non-blocking reverse-DNS cache. Lookups run on background workers.
pub struct HostnameCache {
    resolved: Arc<Mutex<HashMap<IpAddr, Option<String>>>>,
    pending: HashSet<IpAddr>,
    tx: Sender<IpAddr>,
    rx_done: Receiver<(IpAddr, Option<String>)>,
    last_purge: Instant,
}

impl HostnameCache {
    pub fn new() -> Self {
        let resolved = Arc::new(Mutex::new(HashMap::new()));
        let (job_tx, job_rx) = mpsc::channel::<IpAddr>();
        let (done_tx, done_rx) = mpsc::channel::<(IpAddr, Option<String>)>();
        let job_rx = Arc::new(Mutex::new(job_rx));

        for i in 0..3 {
            let job_rx = Arc::clone(&job_rx);
            let done_tx = done_tx.clone();
            thread::Builder::new()
                .name(format!("dns-lookup-{i}"))
                .spawn(move || loop {
                    let ip = {
                        let guard = match job_rx.lock() {
                            Ok(g) => g,
                            Err(_) => break,
                        };
                        match guard.recv() {
                            Ok(ip) => ip,
                            Err(_) => break,
                        }
                    };
                    let name = reverse_lookup(ip);
                    if done_tx.send((ip, name)).is_err() {
                        break;
                    }
                })
                .expect("spawn dns worker");
        }

        Self {
            resolved,
            pending: HashSet::new(),
            tx: job_tx,
            rx_done: done_rx,
            last_purge: Instant::now(),
        }
    }

    pub fn request(&mut self, ip: IpAddr) {
        self.drain_completed();
        if self.pending.contains(&ip) {
            return;
        }
        if let Ok(map) = self.resolved.lock() {
            if map.contains_key(&ip) {
                return;
            }
        }
        self.pending.insert(ip);
        let _ = self.tx.send(ip);
    }

    pub fn get(&mut self, ip: IpAddr) -> Option<String> {
        self.drain_completed();
        self.resolved
            .lock()
            .ok()
            .and_then(|m| m.get(&ip).cloned().flatten())
    }

    fn drain_completed(&mut self) {
        while let Ok((ip, name)) = self.rx_done.try_recv() {
            self.pending.remove(&ip);
            if let Ok(mut map) = self.resolved.lock() {
                map.insert(ip, name);
            }
        }
        if self.last_purge.elapsed() > Duration::from_secs(120) {
            if let Ok(mut map) = self.resolved.lock() {
                if map.len() > 4096 {
                    map.clear();
                }
            }
            self.last_purge = Instant::now();
        }
    }
}

fn reverse_lookup(ip: IpAddr) -> Option<String> {
    match dns_lookup::lookup_addr(&ip) {
        Ok(name) if !name.is_empty() && name != ip.to_string() => Some(name),
        _ => None,
    }
}

impl Default for HostnameCache {
    fn default() -> Self {
        Self::new()
    }
}
