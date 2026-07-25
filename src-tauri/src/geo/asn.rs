//! GeoLite2-ASN lookups (optional offline DB).

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct AsnInfo {
    pub asn: u32,
    pub org: String,
}

pub struct AsnDb {
    reader: Option<maxminddb::Reader<Vec<u8>>>,
    path: Option<PathBuf>,
    cache: Mutex<std::collections::HashMap<IpAddr, Option<AsnInfo>>>,
}

impl AsnDb {
    pub fn new() -> Self {
        let path = find_asn_mmdb();
        let reader = path.as_ref().and_then(|p| {
            match maxminddb::Reader::open_readfile(p) {
                Ok(r) => {
                    eprintln!("[geo] loaded MaxMind ASN DB: {}", p.display());
                    Some(r)
                }
                Err(e) => {
                    eprintln!("[geo] failed to open ASN {}: {e}", p.display());
                    None
                }
            }
        });
        if reader.is_none() {
            eprintln!("[geo] no GeoLite2-ASN.mmdb found (optional)");
        }
        Self {
            reader,
            path,
            cache: Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn loaded(&self) -> bool {
        self.reader.is_some()
    }

    pub fn path_display(&self) -> Option<String> {
        self.path.as_ref().map(|p| p.display().to_string())
    }

    pub fn lookup(&self, ip: IpAddr) -> Option<AsnInfo> {
        if let Ok(cache) = self.cache.lock() {
            if let Some(v) = cache.get(&ip) {
                return v.clone();
            }
        }
        let info = self.lookup_uncached(ip);
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(ip, info.clone());
            if cache.len() > 16_000 {
                cache.clear();
            }
        }
        info
    }

    fn lookup_uncached(&self, ip: IpAddr) -> Option<AsnInfo> {
        let reader = self.reader.as_ref()?;
        #[derive(Deserialize)]
        struct AsnRecord {
            autonomous_system_number: Option<u32>,
            autonomous_system_organization: Option<String>,
        }
        let rec: AsnRecord = reader.lookup(ip).ok()?;
        let asn = rec.autonomous_system_number?;
        let org = rec
            .autonomous_system_organization
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("AS{asn}"));
        Some(AsnInfo { asn, org })
    }
}

impl Default for AsnDb {
    fn default() -> Self {
        Self::new()
    }
}

fn find_asn_mmdb() -> Option<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();

    if let Ok(p) = std::env::var("HOPGLOBE_ASN_MMDB") {
        paths.push(PathBuf::from(p));
    }
    // Sibling ASN next to city DB path
    if let Ok(p) = std::env::var("HOPGLOBE_MMDB") {
        let p = PathBuf::from(p);
        if let Some(parent) = p.parent() {
            paths.push(parent.join("GeoLite2-ASN.mmdb"));
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        for base in [
            cwd.clone(),
            cwd.join("data"),
            cwd.join(".."),
            cwd.join("../data"),
        ] {
            paths.push(base.join("GeoLite2-ASN.mmdb"));
        }
        if let Some(parent) = cwd.parent() {
            paths.push(parent.join("GeoLite2-ASN.mmdb"));
            paths.push(parent.join("data/GeoLite2-ASN.mmdb"));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join("GeoLite2-ASN.mmdb"));
            paths.push(dir.join("../../../GeoLite2-ASN.mmdb"));
            paths.push(dir.join("../../../../GeoLite2-ASN.mmdb"));
            paths.push(dir.join("../../../data/GeoLite2-ASN.mmdb"));
        }
    }

    #[cfg(unix)]
    {
        if let Ok(home) = std::env::var("HOME") {
            let home = PathBuf::from(home);
            paths.push(home.join(".local/share/GeoIP/GeoLite2-ASN.mmdb"));
            paths.push(home.join("GeoLite2-ASN.mmdb"));
            #[cfg(target_os = "macos")]
            {
                paths.push(home.join("Library/Application Support/GeoIP/GeoLite2-ASN.mmdb"));
                paths.push(home.join("Library/Application Support/hopglobe/GeoLite2-ASN.mmdb"));
            }
        }
        paths.push(PathBuf::from("/usr/share/GeoIP/GeoLite2-ASN.mmdb"));
        paths.push(PathBuf::from("/var/lib/GeoIP/GeoLite2-ASN.mmdb"));
    }

    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let local = PathBuf::from(local);
            paths.push(local.join("GeoIP").join("GeoLite2-ASN.mmdb"));
            paths.push(local.join("hopglobe").join("GeoLite2-ASN.mmdb"));
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            paths.push(PathBuf::from(profile).join("GeoLite2-ASN.mmdb"));
        }
    }

    for p in paths {
        if let Ok(c) = p.canonicalize() {
            if c.is_file() {
                return Some(c);
            }
        } else if p.is_file() {
            return Some(p);
        }
    }
    None
}
