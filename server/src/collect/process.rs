use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::model::ProcessIdentity;

const DEAD_PROCESS_TTL: Duration = Duration::from_secs(5);

struct ProcessCache {
    system: System,
    map: HashMap<u32, CachedProcess>,
    last_refresh: Instant,
}

#[derive(Clone)]
struct CachedProcess {
    pid: u32,
    start_time: u64,
    parent_pid: Option<u32>,
    name: String,
    path: Option<PathBuf>,
    user: String,
    session: Option<u32>,
    app_hint: Option<String>,
    last_seen: Instant,
}

impl ProcessCache {
    fn new() -> Self {
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::All, true);
        let mut cache = Self {
            system,
            map: HashMap::new(),
            last_refresh: Instant::now(),
        };
        cache.rebuild_map();
        cache
    }

    fn rebuild_map(&mut self) {
        let now = Instant::now();
        for (pid, process) in self.system.processes() {
            let pid = pid.as_u32();
            self.map.insert(
                pid,
                CachedProcess {
                    pid,
                    start_time: process.start_time(),
                    parent_pid: process.parent().map(Pid::as_u32),
                    name: process.name().to_string_lossy().into_owned(),
                    path: process.exe().map(Path::to_path_buf),
                    user: format!("{:?}", process.user_id()),
                    session: process.session_id().map(Pid::as_u32),
                    app_hint: platform_app_hint(pid),
                    last_seen: now,
                },
            );
        }
        self.map
            .retain(|_, entry| now.duration_since(entry.last_seen) <= DEAD_PROCESS_TTL);
    }

    fn refresh(&mut self) {
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        self.rebuild_map();
        self.last_refresh = Instant::now();
    }

    fn refresh_if_needed(&mut self) {
        if self.last_refresh.elapsed() >= Duration::from_millis(750) {
            self.refresh();
        }
    }

    fn lookup(&mut self, pid: u32) -> ProcessIdentity {
        self.refresh_if_needed();
        if !self.map.contains_key(&pid) {
            self.system
                .refresh_processes(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
            self.rebuild_map();
        }

        let Some(owner) = self.map.get(&pid).cloned() else {
            return unknown_process(pid);
        };
        let root = self.app_root(&owner);
        let app_path = bundle_root(root.path.as_deref()).or_else(|| root.path.clone());
        let app_id = root
            .app_hint
            .clone()
            .or_else(|| {
                app_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("pid:{}:{}", root.pid, root.start_time));
        let app_name = app_path
            .as_deref()
            .and_then(app_display_name)
            .unwrap_or_else(|| root.name.clone());

        ProcessIdentity {
            id: format!("{}:{}", owner.pid, owner.start_time),
            pid: owner.pid,
            start_time: owner.start_time,
            name: owner.name,
            path: owner.path,
            parent_pid: owner.parent_pid,
            app_id,
            app_name,
            app_path,
            is_app_root: root.pid == pid,
        }
    }

    fn app_root(&self, owner: &CachedProcess) -> CachedProcess {
        let mut current = owner.clone();
        for _ in 0..32 {
            let Some(parent_pid) = current.parent_pid else {
                break;
            };
            let Some(parent) = self.map.get(&parent_pid) else {
                break;
            };
            if parent.user != current.user || parent.session != current.session {
                break;
            }
            if !same_application_family(&current, parent) {
                break;
            }
            current = parent.clone();
        }
        current
    }
}

fn same_application_family(child: &CachedProcess, parent: &CachedProcess) -> bool {
    if child.app_hint.is_some() && child.app_hint == parent.app_hint {
        return true;
    }
    match (child.path.as_deref(), parent.path.as_deref()) {
        (Some(child), Some(parent)) if child == parent => true,
        (Some(child), Some(parent)) => {
            let child_bundle = bundle_root(Some(child));
            let parent_bundle = bundle_root(Some(parent));
            if child_bundle.is_some() && child_bundle == parent_bundle {
                return true;
            }
            child.parent() == parent.parent()
                && child.parent().is_some_and(|dir| !is_system_binary_dir(dir))
        }
        _ => false,
    }
}

fn is_system_binary_dir(path: &Path) -> bool {
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "/bin" | "/sbin" | "/usr/bin" | "/usr/sbin" | "/usr/lib" | "/usr/libexec"
    ) || normalized.contains("/windows/system32")
        || normalized.ends_with("/windows")
}

fn bundle_root(path: Option<&Path>) -> Option<PathBuf> {
    let path = path?;
    let mut root = PathBuf::new();
    for component in path.components() {
        root.push(component.as_os_str());
        if component
            .as_os_str()
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(".app")
        {
            return Some(root);
        }
    }
    None
}

fn app_display_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy();
    Some(name.strip_suffix(".app").unwrap_or(&name).to_string())
}

#[cfg(target_os = "linux")]
fn platform_app_hint(pid: u32) -> Option<String> {
    let value = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    value.lines().find_map(|line| {
        let scope = line.rsplit(':').next()?.rsplit('/').next()?;
        ((scope.starts_with("app-") || scope.contains("flatpak") || scope.contains("snap."))
            && scope.ends_with(".scope"))
        .then(|| format!("linux-scope:{scope}"))
    })
}

#[cfg(not(target_os = "linux"))]
fn platform_app_hint(_pid: u32) -> Option<String> {
    None
}

fn unknown_process(pid: u32) -> ProcessIdentity {
    ProcessIdentity {
        id: format!("{pid}:0"),
        pid,
        start_time: 0,
        name: format!("pid {pid}"),
        path: None,
        parent_pid: None,
        app_id: format!("pid:{pid}"),
        app_name: format!("pid {pid}"),
        app_path: None,
        is_app_root: true,
    }
}

fn cache() -> &'static Mutex<ProcessCache> {
    static CACHE: OnceLock<Mutex<ProcessCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ProcessCache::new()))
}

pub fn refresh() {
    if let Ok(mut cache) = cache().lock() {
        cache.refresh();
    }
}

pub fn resolve_info(pid: u32) -> ProcessIdentity {
    cache()
        .lock()
        .map(|mut cache| cache.lookup(pid))
        .unwrap_or_else(|_| unknown_process(pid))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cached(pid: u32, parent: Option<u32>, path: &str) -> CachedProcess {
        CachedProcess {
            pid,
            start_time: pid as u64,
            parent_pid: parent,
            name: Path::new(path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            path: Some(path.into()),
            user: "user".into(),
            session: Some(1),
            app_hint: None,
            last_seen: Instant::now(),
        }
    }

    #[test]
    fn helpers_in_same_app_bundle_share_family() {
        let child = cached(
            2,
            Some(1),
            "/Applications/Browser.app/Contents/Frameworks/helper",
        );
        let parent = cached(1, None, "/Applications/Browser.app/Contents/MacOS/Browser");
        assert!(same_application_family(&child, &parent));
    }

    #[test]
    fn command_does_not_collapse_into_shell() {
        let child = cached(2, Some(1), "/usr/bin/curl");
        let parent = cached(1, None, "/usr/bin/zsh");
        assert!(!same_application_family(&child, &parent));
    }
}
