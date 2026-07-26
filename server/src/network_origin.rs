use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

const DEFAULT_EGRESS_URL: &str = "https://mapmy.network/api/v1/egress";
const USER_AGENT: &str = concat!(
    "netcart/",
    env!("CARGO_PKG_VERSION"),
    " (+https://mapmy.network)"
);

#[derive(Debug, Clone)]
pub struct ExitObservation {
    pub ip: String,
    pub city: Option<String>,
    pub country: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub asn: Option<u32>,
    pub organization: Option<String>,
    pub confidence: Option<f64>,
    pub observed_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginEvidence {
    pub kind: &'static str,
    pub strength: &'static str,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct NetworkOriginView {
    pub hosted: Option<ExitObservation>,
    pub hosted_attempted: bool,
    pub assessment: &'static str,
    pub evidence: Vec<OriginEvidence>,
}

#[derive(Debug)]
struct CachedOrigin {
    hosted: Option<ExitObservation>,
    hosted_attempted: bool,
    assessment: &'static str,
    evidence: Vec<OriginEvidence>,
    signature: String,
}

pub struct NetworkOrigin {
    state: Mutex<CachedOrigin>,
}

impl NetworkOrigin {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(CachedOrigin {
                hosted: None,
                hosted_attempted: false,
                assessment: "unknown",
                evidence: Vec::new(),
                signature: String::new(),
            }),
        }
    }

    /// Refresh cheap local evidence. Returns true when the network route or
    /// proxy configuration changed and the public exit should be rechecked.
    pub fn refresh_local(&self) -> bool {
        let local = inspect_local_routing();
        let signature = local.signature();
        if let Ok(mut state) = self.state.lock() {
            let changed = !state.signature.is_empty() && state.signature != signature;
            state.assessment = local.assessment;
            state.evidence = local.evidence;
            state.signature = signature;
            changed
        } else {
            false
        }
    }

    pub fn refresh_hosted(&self) -> Result<(), String> {
        let result = fetch_egress();
        if let Ok(mut state) = self.state.lock() {
            state.hosted_attempted = true;
            match result {
                Ok(exit) => {
                    state.hosted = Some(exit);
                    Ok(())
                }
                Err(error) => Err(error),
            }
        } else {
            Err("network-origin cache unavailable".into())
        }
    }

    pub fn invalidate_hosted(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.hosted = None;
            state.hosted_attempted = false;
        }
    }

    pub fn view(&self, allow_hosted: bool) -> NetworkOriginView {
        self.state
            .lock()
            .map(|state| NetworkOriginView {
                hosted: allow_hosted.then(|| state.hosted.clone()).flatten(),
                hosted_attempted: allow_hosted && state.hosted_attempted,
                assessment: state.assessment,
                evidence: state.evidence.clone(),
            })
            .unwrap_or(NetworkOriginView {
                hosted: None,
                hosted_attempted: false,
                assessment: "unknown",
                evidence: Vec::new(),
            })
    }
}

impl Default for NetworkOrigin {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct LocalRouting {
    assessment: &'static str,
    evidence: Vec<OriginEvidence>,
}

impl LocalRouting {
    fn signature(&self) -> String {
        format!(
            "{}|{}",
            self.assessment,
            self.evidence
                .iter()
                .map(|item| format!("{}:{}", item.kind, item.label))
                .collect::<Vec<_>>()
                .join("|")
        )
    }
}

fn inspect_local_routing() -> LocalRouting {
    let mut evidence = Vec::new();
    let mut available = false;
    let mut tunnel = false;

    if let Ok(interface) = default_net::get_default_interface() {
        available = true;
        let mut descriptions = vec![interface.name.clone()];
        if let Some(name) = interface.friendly_name {
            descriptions.push(name);
        }
        if let Some(description) = interface.description {
            descriptions.push(description);
        }
        descriptions.sort();
        descriptions.dedup();
        let display = descriptions.join(" · ");
        tunnel = descriptions.iter().any(|value| looks_like_tunnel(value));
        evidence.push(OriginEvidence {
            kind: "default_interface",
            strength: if tunnel { "strong" } else { "supporting" },
            label: if tunnel {
                format!("Default route uses tunnel interface {display}")
            } else {
                format!("Default route uses {display}")
            },
        });
    }

    let env_proxy = proxy_environment();
    if !env_proxy.is_empty() {
        available = true;
        evidence.push(OriginEvidence {
            kind: "environment_proxy",
            strength: "strong",
            label: format!("Proxy configured in {}", env_proxy.join(", ")),
        });
    }
    let system_proxy = system_proxy_enabled();
    if system_proxy {
        available = true;
        evidence.push(OriginEvidence {
            kind: "system_proxy",
            strength: "strong",
            label: "Operating-system proxy is enabled".into(),
        });
    }

    let proxy = system_proxy || !env_proxy.is_empty();
    let assessment = classify_assessment(proxy, tunnel, available);
    LocalRouting {
        assessment,
        evidence,
    }
}

fn classify_assessment(proxy: bool, tunnel: bool, available: bool) -> &'static str {
    match (proxy, tunnel, available) {
        (true, true, _) => "proxy_and_tunnel",
        (true, false, _) => "proxy_configured",
        (false, true, _) => "tunnel_likely",
        (false, false, true) => "no_evidence",
        _ => "unknown",
    }
}

fn looks_like_tunnel(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    let compact = value.replace([' ', '-', '_'], "");
    [
        "utun",
        "wintun",
        "wireguard",
        "nordlynx",
        "tailscale",
        "zerotier",
        "openvpn",
        "protonvpn",
        "mullvad",
        "tunnelblick",
        "tapwindows",
    ]
    .iter()
    .any(|needle| compact.contains(needle))
        || value.starts_with("tun")
        || value.starts_with("tap")
        || value.starts_with("wg")
}

fn proxy_environment() -> Vec<String> {
    let environment: BTreeMap<String, String> = std::env::vars()
        .map(|(key, value)| (key.to_ascii_uppercase(), value))
        .collect();
    proxy_environment_from(&environment)
}

fn proxy_environment_from(environment: &BTreeMap<String, String>) -> Vec<String> {
    ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"]
        .into_iter()
        .filter(|key| {
            environment
                .get(*key)
                .is_some_and(|value| !value.trim().is_empty())
        })
        .map(str::to_string)
        .collect()
}

#[cfg(target_os = "macos")]
fn system_proxy_enabled() -> bool {
    command_output("scutil", &["--proxy"]).is_some_and(|out| {
        ["HTTPEnable : 1", "HTTPSEnable : 1", "SOCKSEnable : 1"]
            .iter()
            .any(|key| out.contains(key))
    })
}

#[cfg(target_os = "windows")]
fn system_proxy_enabled() -> bool {
    command_output(
        "reg",
        &[
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyEnable",
        ],
    )
    .is_some_and(|out| out.to_ascii_lowercase().contains("0x1"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn system_proxy_enabled() -> bool {
    command_output("gsettings", &["get", "org.gnome.system.proxy", "mode"]).is_some_and(|out| {
        let mode = out.trim().trim_matches('\'').to_ascii_lowercase();
        mode == "manual" || mode == "auto"
    })
}

#[cfg(not(any(unix, target_os = "windows")))]
fn system_proxy_enabled() -> bool {
    false
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    let success = loop {
        match child.try_wait().ok()? {
            Some(status) => break status.success(),
            None if started.elapsed() < Duration::from_secs(2) => {
                std::thread::sleep(Duration::from_millis(20));
            }
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !success {
        return None;
    }
    let mut output = String::new();
    child.stdout.take()?.read_to_string(&mut output).ok()?;
    let _ = child.wait();
    Some(output)
}

#[derive(Debug, Deserialize)]
struct EgressResponse {
    egress: HostedEgress,
}

#[derive(Debug, Deserialize)]
struct HostedEgress {
    ip: String,
    city: Option<String>,
    country: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    asn: Option<u32>,
    organization: Option<String>,
    confidence: Option<String>,
}

fn fetch_egress() -> Result<ExitObservation, String> {
    let response = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(6))
        .user_agent(USER_AGENT)
        .build()
        .get(&egress_url())
        .set("Accept", "application/json")
        .call()
        .map_err(|error| format!("egress lookup failed: {error}"))?;
    let parsed: EgressResponse = response
        .into_json()
        .map_err(|error| format!("invalid egress response: {error}"))?;
    let row = parsed.egress;
    Ok(ExitObservation {
        ip: row.ip,
        city: row.city,
        country: row.country,
        lat: row.latitude.filter(|value| value.is_finite()),
        lon: row.longitude.filter(|value| value.is_finite()),
        asn: row.asn,
        organization: row.organization,
        confidence: confidence_from_label(row.confidence.as_deref()),
        observed_at: Instant::now(),
    })
}

fn egress_url() -> String {
    if let Ok(url) = std::env::var("NETWORK_CARTOGRAPHER_EGRESS_URL") {
        return url;
    }
    for key in ["NETWORK_CARTOGRAPHER_GEO_URL", "NETCART_GEO_URL"] {
        if let Ok(url) = std::env::var(key) {
            if let Some(prefix) = url.strip_suffix("/geo") {
                return format!("{prefix}/egress");
            }
        }
    }
    DEFAULT_EGRESS_URL.into()
}

fn confidence_from_label(label: Option<&str>) -> Option<f64> {
    match label.map(|value| value.to_ascii_lowercase()).as_deref() {
        Some("high") => Some(0.82),
        Some("medium") => Some(0.74),
        Some("low") => Some(0.55),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_common_tunnel_interfaces_without_matching_ethernet() {
        for value in [
            "utun4",
            "WireGuard Tunnel",
            "NordLynx",
            "wg0",
            "TAP-Windows Adapter",
        ] {
            assert!(looks_like_tunnel(value), "{value}");
        }
        for value in ["en0", "Ethernet", "Wi-Fi", "wlan0"] {
            assert!(!looks_like_tunnel(value), "{value}");
        }
    }

    #[test]
    fn proxy_evidence_reports_names_but_never_values() {
        let environment = BTreeMap::from([
            (
                "HTTPS_PROXY".into(),
                "https://user:secret@example.test".into(),
            ),
            ("NO_PROXY".into(), "localhost".into()),
        ]);
        assert_eq!(proxy_environment_from(&environment), vec!["HTTPS_PROXY"]);
    }

    #[test]
    fn assessment_uses_explicit_evidence_and_stays_conservative() {
        assert_eq!(classify_assessment(true, true, true), "proxy_and_tunnel");
        assert_eq!(classify_assessment(true, false, true), "proxy_configured");
        assert_eq!(classify_assessment(false, true, true), "tunnel_likely");
        assert_eq!(classify_assessment(false, false, true), "no_evidence");
        assert_eq!(classify_assessment(false, false, false), "unknown");
    }

    #[test]
    fn derives_sibling_egress_url() {
        assert_eq!(
            "https://example.test/api/v1/geo"
                .strip_suffix("/geo")
                .map(|prefix| format!("{prefix}/egress")),
            Some("https://example.test/api/v1/egress".into())
        );
    }
}
