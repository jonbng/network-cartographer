use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use sysinfo::{Pid, ProcessesToUpdate, System};

struct ProcessCache {
    system: System,
    /// pid -> (name, path)
    map: HashMap<u32, (String, Option<PathBuf>)>,
    last_refresh: std::time::Instant,
}

impl ProcessCache {
    fn new() -> Self {
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::All, true);
        let mut cache = Self {
            system,
            map: HashMap::new(),
            last_refresh: std::time::Instant::now(),
        };
        cache.rebuild_map();
        cache
    }

    fn rebuild_map(&mut self) {
        self.map.clear();
        for (pid, proc_) in self.system.processes() {
            let pid = pid.as_u32();
            let name = proc_.name().to_string_lossy().into_owned();
            let path = proc_.exe().map(|p| p.to_path_buf());
            self.map.insert(pid, (name, path));
        }
    }

    fn refresh_if_needed(&mut self) {
        // Process table changes less often than sockets; 2s is fine.
        if self.last_refresh.elapsed() >= std::time::Duration::from_secs(2) {
            self.system.refresh_processes(ProcessesToUpdate::All, true);
            self.rebuild_map();
            self.last_refresh = std::time::Instant::now();
        }
    }

    fn lookup(&mut self, pid: u32) -> (String, Option<PathBuf>) {
        self.refresh_if_needed();
        if let Some(entry) = self.map.get(&pid) {
            return entry.clone();
        }
        // Try a targeted refresh for this pid
        self.system
            .refresh_processes(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
        if let Some(proc_) = self.system.process(Pid::from_u32(pid)) {
            let name = proc_.name().to_string_lossy().into_owned();
            let path = proc_.exe().map(|p| p.to_path_buf());
            self.map.insert(pid, (name.clone(), path.clone()));
            return (name, path);
        }
        ("unknown".into(), None)
    }
}

fn cache() -> &'static Mutex<ProcessCache> {
    static CACHE: OnceLock<Mutex<ProcessCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ProcessCache::new()))
}

pub fn resolve(pid: u32) -> (String, Option<PathBuf>) {
    match cache().lock() {
        Ok(mut c) => c.lookup(pid),
        Err(_) => ("unknown".into(), None),
    }
}
