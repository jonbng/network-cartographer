//! Local, unprivileged destination-name observations.

#[cfg(target_os = "windows")]
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
#[cfg(target_os = "windows")]
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::model::{
    Connection, ConnectionObservation, DestinationName, DestinationNameSource, Protocol,
};

const MAX_TTL: Duration = Duration::from_secs(600);

#[derive(Debug, Clone)]
struct Observation {
    name: DestinationName,
    remote_ip: IpAddr,
    remote_port: Option<u16>,
    pid: Option<u32>,
    local: Option<SocketAddr>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SniObservation {
    pub hostname: String,
    pub remote_ip: IpAddr,
    pub remote_port: Option<u16>,
    pub pid: Option<u32>,
    pub local_ip: Option<IpAddr>,
    pub local_port: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct DestinationNamingStatus {
    pub status: &'static str,
    pub sources: Vec<&'static str>,
    pub message: String,
}

pub struct DestinationNameCache {
    entries: Mutex<Vec<Observation>>,
}

impl DestinationNameCache {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    pub fn record_sni(&self, observation: SniObservation) -> Result<(), String> {
        let hostname = normalize_hostname(&observation.hostname)?;
        if observation.remote_ip.is_unspecified() {
            return Err("remoteIp must identify a remote host".into());
        }
        if observation.remote_port == Some(0) {
            return Err("remotePort must be non-zero".into());
        }
        let local = match (observation.local_ip, observation.local_port) {
            (Some(_), Some(0)) => return Err("localPort must be non-zero".into()),
            (Some(ip), Some(port)) => Some(SocketAddr::new(ip, port)),
            (None, None) => None,
            _ => return Err("localIp and localPort must be supplied together".into()),
        };
        self.insert(Observation {
            name: timed_name(hostname, DestinationNameSource::TlsSni, MAX_TTL),
            remote_ip: observation.remote_ip,
            remote_port: observation.remote_port,
            pid: observation.pid,
            local,
        });
        Ok(())
    }

    #[cfg(any(target_os = "windows", test))]
    pub fn record_dns(&self, hostname: &str, ip: IpAddr, ttl: Duration) -> Result<(), String> {
        let hostname = normalize_hostname(hostname)?;
        self.insert(Observation {
            name: timed_name(
                hostname,
                DestinationNameSource::OsDns,
                ttl.min(MAX_TTL).max(Duration::from_secs(1)),
            ),
            remote_ip: ip,
            remote_port: None,
            pid: None,
            local: None,
        });
        Ok(())
    }

    fn insert(&self, observation: Observation) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        let now = Instant::now();
        entries.retain(|entry| !entry.name.is_expired(now));
        entries.push(observation);
        if entries.len() > 4096 {
            let drain = entries.len() - 4096;
            entries.drain(..drain);
        }
    }

    pub fn enrich(&self, observations: &mut [ConnectionObservation]) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        let now = Instant::now();
        entries.retain(|entry| !entry.name.is_expired(now));
        for observation in observations {
            observation.connection.destination_name = best_match(&entries, &observation.connection);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }
}

impl Default for DestinationNameCache {
    fn default() -> Self {
        Self::new()
    }
}

fn timed_name(value: String, source: DestinationNameSource, ttl: Duration) -> DestinationName {
    let observed_at = Instant::now();
    DestinationName {
        value,
        source,
        observed_at,
        expires_at: observed_at + ttl,
    }
}

fn best_match(entries: &[Observation], connection: &Connection) -> Option<DestinationName> {
    entries
        .iter()
        .filter_map(|entry| match_score(entry, connection).map(|score| (score, entry)))
        .max_by(|(left_score, left), (right_score, right)| {
            left_score
                .cmp(right_score)
                .then_with(|| left.name.observed_at.cmp(&right.name.observed_at))
        })
        .map(|(_, entry)| entry.name.clone())
}

fn match_score(entry: &Observation, connection: &Connection) -> Option<u16> {
    if entry.remote_ip != connection.remote.ip() {
        return None;
    }
    if entry
        .remote_port
        .is_some_and(|port| port != connection.remote.port())
    {
        return None;
    }
    if entry.pid.is_some_and(|pid| {
        connection.pid != Some(pid)
            && !connection
                .processes
                .iter()
                .any(|process| process.pid == pid)
    }) {
        return None;
    }
    if entry.local.is_some_and(|local| local != connection.local) {
        return None;
    }
    if entry.name.source == DestinationNameSource::TlsSni && connection.protocol != Protocol::Tcp {
        return None;
    }

    let source = u16::from(entry.name.source.priority()) * 100;
    let specificity = u16::from(entry.remote_port.is_some())
        + u16::from(entry.pid.is_some()) * 2
        + u16::from(entry.local.is_some()) * 4;
    Some(source + specificity)
}

pub fn normalize_hostname(value: &str) -> Result<String, String> {
    let hostname = value.trim().trim_end_matches('.').to_ascii_lowercase();
    if hostname.is_empty() || hostname.len() > 253 || hostname.parse::<IpAddr>().is_ok() {
        return Err("hostname must be a DNS name".into());
    }
    for label in hostname.split('.') {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err("hostname contains an invalid DNS label".into());
        }
    }
    Ok(hostname)
}

#[derive(Default)]
pub struct OsDnsCollector {
    #[cfg(target_os = "windows")]
    initialized: bool,
    #[cfg(target_os = "windows")]
    previous: HashMap<(String, IpAddr), u64>,
    #[cfg(target_os = "windows")]
    last_error: Option<String>,
}

impl OsDnsCollector {
    pub fn poll(&mut self, cache: &DestinationNameCache) {
        #[cfg(target_os = "windows")]
        {
            match windows_dns_cache() {
                Ok(records) => {
                    let current: HashMap<_, _> = records
                        .iter()
                        .map(|record| ((record.name.clone(), record.ip), record.ttl))
                        .collect();
                    if self.initialized {
                        for record in records {
                            let key = (record.name.clone(), record.ip);
                            let refreshed = self
                                .previous
                                .get(&key)
                                .is_none_or(|previous| record.ttl > previous.saturating_add(2));
                            if refreshed {
                                let _ = cache.record_dns(
                                    &record.name,
                                    record.ip,
                                    Duration::from_secs(record.ttl),
                                );
                            }
                        }
                    }
                    self.previous = current;
                    self.initialized = true;
                    self.last_error = None;
                }
                Err(error) => self.last_error = Some(error),
            }
        }

        #[cfg(not(target_os = "windows"))]
        let _ = cache;
    }

    pub fn status(&self) -> DestinationNamingStatus {
        #[cfg(target_os = "windows")]
        {
            if let Some(error) = &self.last_error {
                return DestinationNamingStatus {
                    status: "degraded",
                    sources: vec!["local-sni-feed"],
                    message: format!("SNI feed available; Windows DNS cache unavailable ({error})"),
                };
            }
            return DestinationNamingStatus {
                status: "ready",
                sources: vec!["local-sni-feed", "windows-dns-cache"],
                message: "Local SNI feed and Windows DNS cache correlation".into(),
            };
        }

        #[cfg(not(target_os = "windows"))]
        DestinationNamingStatus {
            status: "partial",
            sources: vec!["local-sni-feed"],
            message:
                "Local SNI feed available; OS DNS events require elevated access on this platform"
                    .into(),
        }
    }
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct DnsRecord {
    name: String,
    ip: IpAddr,
    ttl: u64,
}

#[cfg(target_os = "windows")]
fn windows_dns_cache() -> Result<Vec<DnsRecord>, String> {
    let script = "Get-DnsClientCache | Where-Object { $_.RecordType -in 1,28 } | ForEach-Object { [pscustomobject]@{ name=$_.Entry; data=$_.Data; ttl=$_.TimeToLive } } | ConvertTo-Json -Compress";
    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    parse_windows_cache(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(any(target_os = "windows", test))]
fn parse_windows_cache(raw: &str) -> Result<Vec<DnsRecord>, String> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    let value: serde_json::Value = serde_json::from_str(raw.trim()).map_err(|e| e.to_string())?;
    let values = match value {
        serde_json::Value::Null => Vec::new(),
        serde_json::Value::Array(values) => values,
        value @ serde_json::Value::Object(_) => vec![value],
        _ => return Err("unexpected Windows DNS cache output".into()),
    };
    let mut records = Vec::new();
    for value in values {
        let Some(name) = value.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(ip) = value
            .get("data")
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse().ok())
        else {
            continue;
        };
        let ttl = value
            .get("ttl")
            .and_then(|value| value.as_u64())
            .unwrap_or(60);
        if let Ok(name) = normalize_hostname(name) {
            records.push(DnsRecord { name, ip, ttl });
        }
    }
    records.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.ip.cmp(&right.ip))
    });
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AttributionSource, ConnState};

    fn connection() -> ConnectionObservation {
        ConnectionObservation {
            active: true,
            connection: Connection {
                pid: Some(42),
                process_name: "browser".into(),
                process_path: None,
                application_id: None,
                processes: Vec::new(),
                local: "127.0.0.1:40000".parse().unwrap(),
                remote: "203.0.113.10:443".parse().unwrap(),
                protocol: Protocol::Tcp,
                state: ConnState::Established,
                attribution: AttributionSource::Direct,
                unattributed_reason: None,
                is_new: true,
                traffic_counters: None,
                destination_name: None,
            },
        }
    }

    #[test]
    fn sni_beats_newer_dns_and_uses_socket_metadata() {
        let cache = DestinationNameCache::new();
        cache
            .record_dns("cdn.example.net", "203.0.113.10".parse().unwrap(), MAX_TTL)
            .unwrap();
        cache
            .record_sni(SniObservation {
                hostname: "www.example.com".into(),
                remote_ip: "203.0.113.10".parse().unwrap(),
                remote_port: Some(443),
                pid: Some(42),
                local_ip: Some("127.0.0.1".parse().unwrap()),
                local_port: Some(40000),
            })
            .unwrap();
        let mut connections = vec![connection()];
        cache.enrich(&mut connections);
        let name = connections[0].connection.destination_name.as_ref().unwrap();
        assert_eq!(name.value, "www.example.com");
        assert_eq!(name.source, DestinationNameSource::TlsSni);
    }

    #[test]
    fn sni_matches_any_process_in_an_application_group() {
        let cache = DestinationNameCache::new();
        cache
            .record_sni(SniObservation {
                hostname: "helper.example.com".into(),
                remote_ip: "203.0.113.10".parse().unwrap(),
                remote_port: Some(443),
                pid: Some(99),
                local_ip: None,
                local_port: None,
            })
            .unwrap();
        let mut observation = connection();
        observation
            .connection
            .processes
            .push(crate::model::ProcessIdentity {
                id: "99:1".into(),
                pid: 99,
                start_time: 1,
                name: "helper".into(),
                path: None,
                parent_pid: None,
                app_id: "browser".into(),
                app_name: "browser".into(),
                app_path: None,
                is_app_root: false,
            });

        cache.enrich(std::slice::from_mut(&mut observation));

        assert_eq!(
            observation
                .connection
                .destination_name
                .as_ref()
                .map(|name| name.value.as_str()),
            Some("helper.example.com")
        );
    }

    #[test]
    fn newest_dns_answer_wins_for_shared_ip() {
        let cache = DestinationNameCache::new();
        cache
            .record_dns("first.example", "203.0.113.10".parse().unwrap(), MAX_TTL)
            .unwrap();
        cache
            .record_dns("second.example", "203.0.113.10".parse().unwrap(), MAX_TTL)
            .unwrap();
        let mut connections = vec![connection()];
        cache.enrich(&mut connections);
        assert_eq!(
            connections[0]
                .connection
                .destination_name
                .as_ref()
                .unwrap()
                .value,
            "second.example"
        );
    }

    #[test]
    fn parses_single_and_multiple_windows_records() {
        let one = parse_windows_cache(r#"{"name":"Example.COM.","data":"203.0.113.1","ttl":30}"#)
            .unwrap();
        assert_eq!(one[0].name, "example.com");
        let many = parse_windows_cache(
            r#"[{"name":"v4.test","data":"192.0.2.1","ttl":10},{"name":"v6.test","data":"2001:db8::1","ttl":20}]"#,
        )
        .unwrap();
        assert_eq!(many.len(), 2);
    }

    #[test]
    fn rejects_invalid_hostnames() {
        assert!(normalize_hostname("https://example.com").is_err());
        assert!(normalize_hostname("203.0.113.1").is_err());
        assert_eq!(
            normalize_hostname("WWW.Example.COM.").unwrap(),
            "www.example.com"
        );
    }
}
