use std::path::Path;

#[cfg(target_os = "macos")]
mod platform {
    use std::collections::HashMap;
    use std::ffi::CString;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    enum IconEntry {
        Unloaded(PathBuf),
        Ready(Arc<[u8]>),
        Missing,
    }

    #[derive(Default)]
    struct IconState {
        ids_by_path: HashMap<PathBuf, String>,
        entries: HashMap<String, IconEntry>,
    }

    #[derive(Default)]
    pub struct AppIconStore {
        state: Mutex<IconState>,
    }

    impl AppIconStore {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn url_for(&self, path: &Path) -> Option<String> {
            let bundle = bundle_path(path)?;
            let mut state = self.state.lock().ok()?;
            if let Some(id) = state.ids_by_path.get(&bundle) {
                return Some(format!("/api/app-icons/{id}.png"));
            }
            let mut random = [0_u8; 16];
            getrandom::getrandom(&mut random).ok()?;
            let id = random.iter().map(|byte| format!("{byte:02x}")).collect();
            state.ids_by_path.insert(bundle.clone(), id.clone());
            state
                .entries
                .insert(id.clone(), IconEntry::Unloaded(bundle));
            Some(format!("/api/app-icons/{id}.png"))
        }

        pub fn get(&self, id: &str) -> Option<Arc<[u8]>> {
            let path = {
                let state = self.state.lock().ok()?;
                match state.entries.get(id)? {
                    IconEntry::Ready(bytes) => return Some(Arc::clone(bytes)),
                    IconEntry::Missing => return None,
                    IconEntry::Unloaded(path) => path.clone(),
                }
            };

            let loaded = extract_png(&path).map(Arc::<[u8]>::from);
            if let Ok(mut state) = self.state.lock() {
                state.entries.insert(
                    id.to_owned(),
                    loaded
                        .as_ref()
                        .map(|bytes| IconEntry::Ready(Arc::clone(bytes)))
                        .unwrap_or(IconEntry::Missing),
                );
            }
            loaded
        }

        pub fn clear(&self) {
            if let Ok(mut state) = self.state.lock() {
                *state = IconState::default();
            }
        }
    }

    fn bundle_path(path: &Path) -> Option<PathBuf> {
        let mut candidate = PathBuf::new();
        for component in path.components() {
            candidate.push(component.as_os_str());
            if component
                .as_os_str()
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(".app")
            {
                return Some(candidate);
            }
        }
        None
    }

    fn extract_png(path: &Path) -> Option<Vec<u8>> {
        unsafe extern "C" {
            fn nc_copy_app_icon_png(
                path: *const std::ffi::c_char,
                output: *mut *mut u8,
                length: *mut usize,
            ) -> i32;
            fn nc_free_buffer(buffer: *mut std::ffi::c_void);
        }

        let path = CString::new(path.to_string_lossy().as_bytes()).ok()?;
        let mut output = std::ptr::null_mut();
        let mut length = 0_usize;
        if unsafe { nc_copy_app_icon_png(path.as_ptr(), &mut output, &mut length) } != 0
            || output.is_null()
            || length == 0
        {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(output, length) }.to_vec();
        unsafe { nc_free_buffer(output.cast()) };
        Some(bytes)
    }

    #[cfg(test)]
    mod tests {
        use super::{bundle_path, AppIconStore};
        use std::path::Path;

        #[test]
        fn finds_bundle_root_inside_executable_path() {
            assert_eq!(
                bundle_path(Path::new("/Applications/Safari.app/Contents/MacOS/Safari")),
                Some(Path::new("/Applications/Safari.app").to_path_buf())
            );
            assert_eq!(bundle_path(Path::new("/usr/bin/curl")), None);
        }

        #[test]
        fn system_bundle_icon_is_png() {
            let finder = Path::new("/System/Library/CoreServices/Finder.app");
            if !finder.exists() {
                return;
            }
            let store = AppIconStore::new();
            let url = store.url_for(finder).expect("register Finder icon");
            let id = url
                .strip_prefix("/api/app-icons/")
                .and_then(|value| value.strip_suffix(".png"))
                .expect("opaque icon URL");
            let bytes = store.get(id).expect("extract Finder icon");
            assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use std::path::Path;
    use std::sync::Arc;

    #[derive(Default)]
    pub struct AppIconStore;

    impl AppIconStore {
        pub fn new() -> Self {
            Self
        }

        pub fn url_for(&self, _path: &Path) -> Option<String> {
            None
        }

        pub fn get(&self, _id: &str) -> Option<Arc<[u8]>> {
            None
        }

        pub fn clear(&self) {}
    }
}

pub use platform::AppIconStore;

pub fn icon_id_from_request(value: &str) -> Option<&str> {
    let id = value.strip_suffix(".png")?;
    (id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(id)
}

pub fn app_icon_url(store: &AppIconStore, path: Option<&Path>) -> Option<String> {
    store.url_for(path?)
}

#[cfg(test)]
mod tests {
    use super::icon_id_from_request;

    #[test]
    fn accepts_only_opaque_png_ids() {
        assert_eq!(
            icon_id_from_request("00112233445566778899aabbccddeeff.png"),
            Some("00112233445566778899aabbccddeeff")
        );
        assert_eq!(icon_id_from_request("../../Safari.app.png"), None);
        assert_eq!(icon_id_from_request("0011.jpg"), None);
    }
}
