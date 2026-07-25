use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    Tcp,
    #[allow(dead_code)]
    Udp,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::Tcp => "TCP",
            Protocol::Udp => "UDP",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    Established,
    Listen,
    Other,
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub pid: Option<u32>,
    pub process_name: String,
    pub process_path: Option<PathBuf>,
    #[allow(dead_code)]
    pub local: SocketAddr,
    pub remote: SocketAddr,
    pub protocol: Protocol,
    #[allow(dead_code)]
    pub state: ConnState,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DestKey {
    pub ip: IpAddr,
    pub port: u16,
    pub protocol: Protocol,
}

impl DestKey {
    pub fn from_remote(remote: SocketAddr, protocol: Protocol) -> Self {
        Self {
            ip: remote.ip(),
            port: remote.port(),
            protocol,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DestStats {
    pub remote: SocketAddr,
    pub hostname: Option<String>,
    /// TLS SNI if captured
    pub sni: Option<String>,
    pub protocol: Protocol,
    pub hit_count: u64,
    #[allow(dead_code)]
    pub first_seen: Instant,
    pub last_seen: Instant,
}

impl DestStats {
    pub fn display_host(&self) -> String {
        self.hostname
            .clone()
            .unwrap_or_else(|| self.remote.ip().to_string())
    }
}

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub name: String,
    pub path: Option<PathBuf>,
    pub pids: std::collections::BTreeSet<u32>,
    pub destinations: std::collections::HashMap<DestKey, DestStats>,
    pub last_seen: Instant,
    /// Hits observed last poll cycle (for activity rate).
    pub hits_prev_cycle: u64,
    /// Approximate hits/sec over last poll interval.
    pub hits_per_sec: f64,
}

impl AppEntry {
    pub fn connection_hits(&self) -> u64 {
        self.destinations.values().map(|d| d.hit_count).sum()
    }

    pub fn sorted_destinations(&self) -> Vec<&DestStats> {
        let mut list: Vec<_> = self.destinations.values().collect();
        list.sort_by(|a, b| {
            b.last_seen
                .cmp(&a.last_seen)
                .then_with(|| b.hit_count.cmp(&a.hit_count))
                .then_with(|| a.display_host().cmp(&b.display_host()))
        });
        list
    }
}

/// True for loopback or typical private/link-local ranges.
pub fn is_local_or_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.octets()[0] == 100 && (v4.octets()[1] & 0b1100_0000) == 0b0100_0000 // CGNAT 100.64/10
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6.is_unspecified()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn private_and_public_v4() {
        assert!(is_local_or_private(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_local_or_private(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_local_or_private(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_local_or_private(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_local_or_private(IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1)))); // CGNAT
        assert!(!is_local_or_private(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(!is_local_or_private(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }

    #[test]
    fn private_and_public_v6() {
        assert!(is_local_or_private(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(is_local_or_private(
            "fd12:3456:789a::1".parse::<IpAddr>().unwrap()
        ));
        assert!(!is_local_or_private(
            "2606:4700:4700::1111".parse::<IpAddr>().unwrap()
        ));
    }
}
