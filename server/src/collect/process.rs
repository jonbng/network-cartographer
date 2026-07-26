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
    app_hint: Option<AppHint>,
    last_seen: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AppHint {
    identity: String,
    desktop_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AppMetadata {
    id: String,
    name: String,
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
            let start_time = process.start_time();
            let app_hint = self
                .map
                .get(&pid)
                .filter(|cached| cached.start_time == start_time)
                .map(|cached| cached.app_hint.clone())
                .unwrap_or_else(|| platform_app_hint(pid));
            let mut entry = CachedProcess {
                pid,
                start_time,
                parent_pid: process.parent().map(Pid::as_u32),
                name: process.name().to_string_lossy().into_owned(),
                path: process.exe().map(Path::to_path_buf),
                user: format!("{:?}", process.user_id()),
                session: process.session_id().map(Pid::as_u32),
                app_hint,
                last_seen: now,
            };
            if entry.path.is_none() || entry.name.trim().is_empty() {
                if let Some(native) = platform_process_fallback(pid) {
                    if entry.path.is_none() {
                        entry.path = native.path;
                    }
                    if entry.name.trim().is_empty() {
                        entry.name = native.name;
                    }
                    if entry.parent_pid.is_none() {
                        entry.parent_pid = native.parent_pid;
                    }
                }
            }
            self.map.insert(pid, entry);
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
            if let std::collections::hash_map::Entry::Vacant(entry) = self.map.entry(pid) {
                if let Some(owner) = platform_process_fallback(pid) {
                    entry.insert(owner);
                }
            }
        }

        let Some(owner) = self.map.get(&pid).cloned() else {
            return unknown_process(pid);
        };
        let root = self.app_root(&owner);
        let app_path = bundle_root(root.path.as_deref()).or_else(|| root.path.clone());
        let metadata = platform_app_metadata(&root, app_path.as_deref());
        let app_id = metadata
            .as_ref()
            .map(|metadata| metadata.id.clone())
            .or_else(|| root.app_hint.as_ref().map(|hint| hint.identity.clone()))
            .or_else(|| {
                app_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("pid:{}:{}", root.pid, root.start_time));
        let app_name = metadata
            .map(|metadata| metadata.name)
            .or_else(|| app_path.as_deref().and_then(app_display_name))
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

    fn is_in_process_tree(&self, pid: u32, root_pid: u32) -> bool {
        let mut current = pid;
        for _ in 0..64 {
            if current == root_pid {
                return true;
            }
            let Some(parent) = self.map.get(&current).and_then(|entry| entry.parent_pid) else {
                return false;
            };
            current = parent;
        }
        false
    }
}

#[cfg(target_os = "macos")]
fn platform_process_fallback(pid: u32) -> Option<CachedProcess> {
    use std::ffi::{c_char, CStr};

    #[repr(C)]
    struct NativeIdentity {
        parent_pid: u32,
        start_time: u64,
        user_id: u32,
        session_id: i32,
        name: [c_char; 256],
        path: [c_char; 4096],
    }

    unsafe extern "C" {
        fn nc_read_process_identity(pid: i32, output: *mut NativeIdentity) -> i32;
    }

    let mut native = NativeIdentity {
        parent_pid: 0,
        start_time: 0,
        user_id: 0,
        session_id: -1,
        name: [0; 256],
        path: [0; 4096],
    };
    if unsafe { nc_read_process_identity(pid as i32, &mut native) } != 0 {
        return None;
    }
    let name = unsafe { CStr::from_ptr(native.name.as_ptr()) }
        .to_string_lossy()
        .trim()
        .to_string();
    let native_path = unsafe { CStr::from_ptr(native.path.as_ptr()) };
    let path = (!native_path.to_bytes().is_empty())
        .then(|| PathBuf::from(native_path.to_string_lossy().as_ref()));
    if name.is_empty() && path.is_none() {
        return None;
    }
    Some(CachedProcess {
        pid,
        start_time: native.start_time,
        parent_pid: (native.parent_pid > 0).then_some(native.parent_pid),
        name: if name.is_empty() {
            path.as_deref()
                .and_then(Path::file_name)
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("pid-{pid}"))
        } else {
            name
        },
        path,
        user: format!("uid:{}", native.user_id),
        session: (native.session_id >= 0).then_some(native.session_id as u32),
        app_hint: platform_app_hint(pid),
        last_seen: Instant::now(),
    })
}

#[cfg(not(target_os = "macos"))]
fn platform_process_fallback(_pid: u32) -> Option<CachedProcess> {
    None
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
fn platform_app_hint(pid: u32) -> Option<AppHint> {
    if let Ok(value) = std::fs::read(format!("/proc/{pid}/environ")) {
        let variables = value.split(|byte| *byte == 0).filter_map(|entry| {
            let separator = entry.iter().position(|byte| *byte == b'=')?;
            Some((&entry[..separator], &entry[separator + 1..]))
        });
        let mut snap_instance = None;
        let mut snap_app = None;
        for (key, value) in variables {
            let value = String::from_utf8_lossy(value);
            let desktop_id = match key {
                b"GIO_LAUNCHED_DESKTOP_FILE" | b"BAMF_DESKTOP_FILE_HINT" => {
                    Path::new(value.as_ref())
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(|name| name.strip_suffix(".desktop").unwrap_or(name).to_string())
                }
                b"FLATPAK_ID" => Some(value.to_string()),
                _ => None,
            };
            if let Some(desktop_id) = desktop_id.filter(|value| !value.is_empty()) {
                return Some(AppHint {
                    identity: format!("linux-app:{desktop_id}"),
                    desktop_id: Some(desktop_id),
                });
            }
            if matches!(key, b"SNAP_INSTANCE_NAME" | b"SNAP_NAME") && !value.is_empty() {
                snap_instance.get_or_insert_with(|| value.to_string());
            } else if key == b"SNAP_APP_NAME" && !value.is_empty() {
                snap_app = Some(value.to_string());
            }
        }
        if let Some(instance) = snap_instance {
            let desktop_id = snap_app.map(|app| format!("{instance}_{app}"));
            return Some(AppHint {
                identity: format!("linux-snap:{instance}"),
                desktop_id,
            });
        }
    }

    let value = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    value.lines().find_map(|line| {
        let scope = line.rsplit(':').next()?.rsplit('/').next()?;
        ((scope.starts_with("app-") || scope.contains("flatpak") || scope.contains("snap."))
            && scope.ends_with(".scope"))
        .then(|| AppHint {
            identity: format!("linux-scope:{scope}"),
            desktop_id: None,
        })
    })
}

#[cfg(not(target_os = "linux"))]
fn platform_app_hint(_pid: u32) -> Option<AppHint> {
    None
}

#[cfg(target_os = "linux")]
fn platform_app_metadata(root: &CachedProcess, _app_path: Option<&Path>) -> Option<AppMetadata> {
    linux_desktop::resolve(root)
}

#[cfg(target_os = "linux")]
mod linux_desktop {
    use super::{AppMetadata, CachedProcess};
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    const REGISTRY_TTL: Duration = Duration::from_secs(30);

    #[derive(Clone, Debug)]
    struct DesktopApp {
        id: String,
        name: String,
        aliases: Vec<String>,
    }

    #[derive(Default)]
    struct DesktopRegistry {
        apps: HashMap<String, DesktopApp>,
        aliases: HashMap<String, Vec<String>>,
    }

    struct RegistryCache {
        registry: DesktopRegistry,
        resolutions: HashMap<String, Option<AppMetadata>>,
        loaded_at: Instant,
    }

    pub(super) fn resolve(root: &CachedProcess) -> Option<AppMetadata> {
        let cache = cache();
        let mut cache = cache.lock().ok()?;
        if cache.loaded_at.elapsed() >= REGISTRY_TTL {
            cache.registry = DesktopRegistry::load();
            cache.resolutions.clear();
            cache.loaded_at = Instant::now();
        }
        let key = root
            .app_hint
            .as_ref()
            .map(|hint| {
                format!(
                    "{}|{}",
                    hint.identity,
                    hint.desktop_id.as_deref().unwrap_or_default()
                )
            })
            .or_else(|| {
                root.path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| root.name.clone());
        if let Some(metadata) = cache.resolutions.get(&key) {
            return metadata.clone();
        }
        let metadata = cache.registry.resolve(root);
        cache.resolutions.insert(key, metadata.clone());
        metadata
    }

    fn cache() -> &'static Mutex<RegistryCache> {
        static CACHE: OnceLock<Mutex<RegistryCache>> = OnceLock::new();
        CACHE.get_or_init(|| {
            Mutex::new(RegistryCache {
                registry: DesktopRegistry::load(),
                resolutions: HashMap::new(),
                loaded_at: Instant::now(),
            })
        })
    }

    impl DesktopRegistry {
        fn load() -> Self {
            let mut registry = Self::default();
            let mut seen = HashSet::new();
            for directory in application_directories() {
                for (id, path) in desktop_files(&directory) {
                    if !seen.insert(id.clone()) {
                        continue;
                    }
                    if let Some(app) = parse_desktop_entry(&id, &path) {
                        registry.insert(app);
                    }
                }
            }
            registry
        }

        fn insert(&mut self, app: DesktopApp) {
            for alias in &app.aliases {
                self.aliases
                    .entry(alias.to_ascii_lowercase())
                    .or_default()
                    .push(app.id.clone());
            }
            self.apps.insert(app.id.clone(), app);
        }

        fn resolve(&self, root: &CachedProcess) -> Option<AppMetadata> {
            if let Some(id) = root
                .app_hint
                .as_ref()
                .and_then(|hint| hint.desktop_id.as_deref())
            {
                if let Some(app) = self.apps.get(id.strip_suffix(".desktop").unwrap_or(id)) {
                    return Some(metadata(app));
                }
            }

            if let Some(hint) = &root.app_hint {
                let scope = systemd_unescape(&hint.identity);
                if let Some(app) = self
                    .apps
                    .values()
                    .filter(|app| scope.contains(&app.id))
                    .max_by_key(|app| app.id.len())
                {
                    return Some(metadata(app));
                }
            }

            let mut aliases = Vec::new();
            if let Some(path) = &root.path {
                aliases.push(path.to_string_lossy().into_owned());
                if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                    aliases.push(name.to_string());
                }
            }
            aliases.push(root.name.clone());
            aliases.into_iter().find_map(|alias| {
                let ids = self.aliases.get(&alias.to_ascii_lowercase())?;
                let mut unique = ids.iter().collect::<Vec<_>>();
                unique.sort_unstable();
                unique.dedup();
                (unique.len() == 1)
                    .then(|| self.apps.get(unique[0]))
                    .flatten()
                    .map(metadata)
            })
        }
    }

    fn metadata(app: &DesktopApp) -> AppMetadata {
        AppMetadata {
            id: format!("linux-desktop:{}", app.id),
            name: app.name.clone(),
        }
    }

    fn application_directories() -> Vec<PathBuf> {
        let mut directories = Vec::new();
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".local/share")));
        if let Some(data_home) = &data_home {
            directories.push(data_home.join("applications"));
            directories.push(data_home.join("flatpak/exports/share/applications"));
        }
        for data_dir in std::env::var_os("XDG_DATA_DIRS")
            .map(|value| std::env::split_paths(&value).collect())
            .unwrap_or_else(|| {
                vec![
                    PathBuf::from("/usr/local/share"),
                    PathBuf::from("/usr/share"),
                ]
            })
        {
            directories.push(data_dir.join("applications"));
        }
        directories.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
        directories.push(PathBuf::from("/var/lib/snapd/desktop/applications"));
        let mut seen = HashSet::new();
        directories.retain(|path| seen.insert(path.clone()));
        directories
    }

    fn desktop_files(root: &Path) -> Vec<(String, PathBuf)> {
        fn visit(root: &Path, directory: &Path, output: &mut Vec<(String, PathBuf)>) {
            let Ok(entries) = std::fs::read_dir(directory) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit(root, &path, output);
                } else if path
                    .extension()
                    .is_some_and(|extension| extension == "desktop")
                {
                    let Ok(relative) = path.strip_prefix(root) else {
                        continue;
                    };
                    let mut id = relative.to_string_lossy().replace(['/', '\\'], "-");
                    if let Some(stripped) = id.strip_suffix(".desktop") {
                        id = stripped.to_string();
                    }
                    output.push((id, path));
                }
            }
        }
        let mut output = Vec::new();
        visit(root, root, &mut output);
        output.sort_by(|left, right| left.0.cmp(&right.0));
        output
    }

    fn parse_desktop_entry(id: &str, path: &Path) -> Option<DesktopApp> {
        let value = std::fs::read_to_string(path).ok()?;
        parse_desktop_value(id, &value)
    }

    fn parse_desktop_value(id: &str, value: &str) -> Option<DesktopApp> {
        let mut in_desktop_entry = false;
        let mut names = HashMap::new();
        let mut aliases = vec![id.to_string()];
        let mut is_application = true;
        let mut hidden = false;
        for raw_line in value.lines() {
            let line = raw_line.trim();
            if line.starts_with('[') && line.ends_with(']') {
                in_desktop_entry = line == "[Desktop Entry]";
                continue;
            }
            if !in_desktop_entry || line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, raw_value)) = line.split_once('=') else {
                continue;
            };
            let value = unescape(raw_value);
            match key {
                "Name" => {
                    names.insert(String::new(), value);
                }
                key if key.starts_with("Name[") && key.ends_with(']') => {
                    names.insert(key[5..key.len() - 1].to_string(), value);
                }
                "Type" => is_application = value == "Application",
                "Hidden" => hidden = value.eq_ignore_ascii_case("true"),
                "StartupWMClass" => aliases.push(value),
                "TryExec" => add_executable_aliases(&mut aliases, &value),
                "Exec" => {
                    if let Some(executable) = first_exec_token(&value) {
                        add_executable_aliases(&mut aliases, &executable);
                    }
                }
                _ => {}
            }
        }
        if hidden || !is_application {
            return None;
        }
        let name = localized_name(&names)?;
        aliases.retain(|alias| !alias.is_empty());
        aliases.sort_unstable();
        aliases.dedup();
        Some(DesktopApp {
            id: id.to_string(),
            name,
            aliases,
        })
    }

    fn localized_name(names: &HashMap<String, String>) -> Option<String> {
        for locale in locale_candidates() {
            if let Some(name) = names.get(&locale).filter(|name| !name.is_empty()) {
                return Some(name.clone());
            }
        }
        names.get("").filter(|name| !name.is_empty()).cloned()
    }

    fn locale_candidates() -> Vec<String> {
        let raw = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
            .unwrap_or_default();
        let locale = raw.split('.').next().unwrap_or("");
        let mut candidates = Vec::new();
        if !locale.is_empty() && locale != "C" && locale != "POSIX" {
            candidates.push(locale.to_string());
            if let Some((without_modifier, _)) = locale.split_once('@') {
                candidates.push(without_modifier.to_string());
            }
            if let Some((language, _)) = locale.split_once('_') {
                candidates.push(language.to_string());
            }
        }
        candidates.dedup();
        candidates
    }

    fn add_executable_aliases(aliases: &mut Vec<String>, executable: &str) {
        let executable = executable.trim();
        if executable.is_empty() {
            return;
        }
        aliases.push(executable.to_string());
        if let Some(name) = Path::new(executable)
            .file_name()
            .and_then(|name| name.to_str())
        {
            aliases.push(name.to_string());
        }
    }

    fn first_exec_token(value: &str) -> Option<String> {
        let mut token = String::new();
        let mut quote = None;
        let mut escaped = false;
        for character in value.chars() {
            if escaped {
                token.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if let Some(active_quote) = quote {
                if character == active_quote {
                    quote = None;
                } else {
                    token.push(character);
                }
            } else if matches!(character, '\'' | '"') {
                quote = Some(character);
            } else if character.is_whitespace() {
                if !token.is_empty() {
                    break;
                }
            } else {
                token.push(character);
            }
        }
        (!token.is_empty() && !token.starts_with('%')).then_some(token)
    }

    fn unescape(value: &str) -> String {
        let mut output = String::with_capacity(value.len());
        let mut characters = value.chars();
        while let Some(character) = characters.next() {
            if character != '\\' {
                output.push(character);
                continue;
            }
            match characters.next() {
                Some('s') => output.push(' '),
                Some('n') => output.push('\n'),
                Some('t') => output.push('\t'),
                Some('r') => output.push('\r'),
                Some('\\') => output.push('\\'),
                Some(other) => output.push(other),
                None => output.push('\\'),
            }
        }
        output
    }

    fn systemd_unescape(value: &str) -> String {
        let bytes = value.as_bytes();
        let mut output = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if index + 3 < bytes.len() && bytes[index] == b'\\' && bytes[index + 1] == b'x' {
                let hex = &value[index + 2..index + 4];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    output.push(byte);
                    index += 4;
                    continue;
                }
            }
            output.push(bytes[index]);
            index += 1;
        }
        String::from_utf8_lossy(&output).into_owned()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::collect::process::{AppHint, CachedProcess};

        fn process(name: &str, hint: Option<AppHint>) -> CachedProcess {
            CachedProcess {
                pid: 7,
                start_time: 1,
                parent_pid: None,
                name: name.into(),
                path: Some(format!("/app/bin/{name}").into()),
                user: "user".into(),
                session: Some(1),
                app_hint: hint,
                last_seen: Instant::now(),
            }
        }

        fn registry(apps: Vec<DesktopApp>) -> DesktopRegistry {
            let mut registry = DesktopRegistry::default();
            for app in apps {
                registry.insert(app);
            }
            registry
        }

        #[test]
        fn flatpak_scope_resolves_zen_browser_name() {
            let app = parse_desktop_value(
                "app.zen_browser.zen",
                "[Desktop Entry]\nType=Application\nName=Zen Browser\nExec=/usr/bin/flatpak run app.zen_browser.zen\nStartupWMClass=zen\n",
            )
            .unwrap();
            let registry = registry(vec![app]);
            let process = process(
                "zen",
                Some(AppHint {
                    identity: "linux-scope:app-flatpak-app.zen_browser.zen-123.scope".into(),
                    desktop_id: None,
                }),
            );
            assert_eq!(
                registry.resolve(&process),
                Some(AppMetadata {
                    id: "linux-desktop:app.zen_browser.zen".into(),
                    name: "Zen Browser".into(),
                })
            );
        }

        #[test]
        fn ambiguous_executable_alias_is_not_guessed() {
            let first = DesktopApp {
                id: "one".into(),
                name: "One".into(),
                aliases: vec!["browser".into()],
            };
            let second = DesktopApp {
                id: "two".into(),
                name: "Two".into(),
                aliases: vec!["browser".into()],
            };
            assert_eq!(
                registry(vec![first, second]).resolve(&process("browser", None)),
                None
            );
        }

        #[test]
        fn desktop_parser_stops_before_action_names() {
            let app = parse_desktop_value(
                "browser",
                "[Desktop Entry]\nName=Friendly Browser\nType=Application\nExec=browser %u\n[Desktop Action new]\nName=New Window\n",
            )
            .unwrap();
            assert_eq!(app.name, "Friendly Browser");
            assert!(app.aliases.contains(&"browser".to_string()));
        }
    }
}

#[cfg(target_os = "macos")]
fn platform_app_metadata(_root: &CachedProcess, app_path: Option<&Path>) -> Option<AppMetadata> {
    use std::collections::HashMap;
    use std::ffi::CString;

    unsafe extern "C" {
        fn nc_bundle_display_name(
            path: *const std::ffi::c_char,
            output: *mut u8,
            capacity: usize,
        ) -> usize;
    }

    let path = app_path?.to_path_buf();
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().ok()?;
    let name = cache
        .entry(path.clone())
        .or_insert_with(|| {
            let value = CString::new(path.to_string_lossy().as_bytes()).ok()?;
            let mut output = [0_u8; 1024];
            let written = unsafe {
                nc_bundle_display_name(value.as_ptr(), output.as_mut_ptr(), output.len())
            };
            (written > 0 && written < output.len())
                .then(|| {
                    String::from_utf8_lossy(&output[..written])
                        .trim()
                        .to_string()
                })
                .filter(|name| !name.is_empty())
        })
        .clone()?;
    Some(AppMetadata {
        id: path.to_string_lossy().into_owned(),
        name,
    })
}

#[cfg(target_os = "windows")]
fn platform_app_metadata(_root: &CachedProcess, app_path: Option<&Path>) -> Option<AppMetadata> {
    use std::collections::HashMap;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn version_name(path: &Path) -> Option<String> {
        let path = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut ignored = 0;
        let size = unsafe { GetFileVersionInfoSizeW(path.as_ptr(), &mut ignored) };
        if size == 0 {
            return None;
        }
        let mut data = vec![0_u8; size as usize];
        if unsafe { GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr().cast()) } == 0 {
            return None;
        }

        let translation_key = wide("\\VarFileInfo\\Translation");
        let mut translations = std::ptr::null_mut();
        let mut translation_bytes = 0;
        let mut language_pairs = Vec::new();
        if unsafe {
            VerQueryValueW(
                data.as_ptr().cast(),
                translation_key.as_ptr(),
                &mut translations,
                &mut translation_bytes,
            )
        } != 0
            && !translations.is_null()
        {
            let values = unsafe {
                std::slice::from_raw_parts(
                    translations.cast::<u16>(),
                    translation_bytes as usize / std::mem::size_of::<u16>(),
                )
            };
            for pair in values.chunks_exact(2) {
                language_pairs.push((pair[0], pair[1]));
            }
        }
        if language_pairs.is_empty() {
            language_pairs.extend([(0x0409, 1200), (0x0409, 1252)]);
        }

        for property in ["FileDescription", "ProductName"] {
            for (language, code_page) in &language_pairs {
                let key = wide(&format!(
                    "\\StringFileInfo\\{language:04x}{code_page:04x}\\{property}"
                ));
                let mut value = std::ptr::null_mut();
                let mut length = 0;
                if unsafe {
                    VerQueryValueW(data.as_ptr().cast(), key.as_ptr(), &mut value, &mut length)
                } == 0
                    || value.is_null()
                    || length == 0
                {
                    continue;
                }
                let value =
                    unsafe { std::slice::from_raw_parts(value.cast::<u16>(), length as usize) };
                let end = value
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(value.len());
                let name = String::from_utf16_lossy(&value[..end]).trim().to_string();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
        None
    }

    let path = app_path?.to_path_buf();
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().ok()?;
    let name = cache
        .entry(path.clone())
        .or_insert_with(|| version_name(&path))
        .clone()?;
    Some(AppMetadata {
        id: path.to_string_lossy().into_owned(),
        name,
    })
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_app_metadata(_root: &CachedProcess, _app_path: Option<&Path>) -> Option<AppMetadata> {
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

/// Remove the monitor and any commands it spawned from an owner PID list.
/// Unknown PIDs are retained so short-lived third-party traffic does not
/// disappear merely because its process exited before attribution.
pub fn retain_outside_process_tree(pids: &mut Vec<u32>, root_pid: u32) {
    if let Ok(cache) = cache().lock() {
        pids.retain(|pid| !cache.is_in_process_tree(*pid, root_pid));
    }
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

    #[test]
    fn spawned_commands_are_recognized_as_part_of_process_tree() {
        let mut map = HashMap::new();
        map.insert(10, cached(10, None, "/opt/netcart"));
        map.insert(11, cached(11, Some(10), "/usr/bin/traceroute"));
        map.insert(12, cached(12, Some(11), "/usr/bin/helper"));
        map.insert(20, cached(20, None, "/usr/bin/browser"));
        let cache = ProcessCache {
            system: System::new(),
            map,
            last_refresh: Instant::now(),
        };

        assert!(cache.is_in_process_tree(10, 10));
        assert!(cache.is_in_process_tree(11, 10));
        assert!(cache.is_in_process_tree(12, 10));
        assert!(!cache.is_in_process_tree(20, 10));
        assert!(!cache.is_in_process_tree(999, 10));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn libproc_fallback_identifies_current_process() {
        let process = platform_process_fallback(std::process::id())
            .expect("libproc should identify the current process");
        assert_eq!(process.pid, std::process::id());
        assert!(!process.name.is_empty());
        assert!(process.path.is_some());
        assert!(process.start_time > 0);
    }
}
