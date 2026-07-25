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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnState {
    Connecting,
    Established,
    Closing,
    TimeWait,
    Listen,
    Closed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributionSource {
    Direct,
    Recovered,
    Unattributed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnattributedReason {
    OwnerGone,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AttributionStats {
    pub direct: usize,
    pub recovered: usize,
    pub unattributed: usize,
    pub ambiguous: usize,
    pub owner_gone: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SocketKey {
    pub protocol: Protocol,
    pub local: SocketAddr,
    pub remote: SocketAddr,
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
    pub attribution: AttributionSource,
    pub unattributed_reason: Option<UnattributedReason>,
    /// True only on the first snapshot in which this exact socket exists.
    pub is_new: bool,
    pub traffic_counters: Option<SocketTrafficCounters>,
}

impl Connection {
    pub fn socket_key(&self) -> SocketKey {
        SocketKey {
            protocol: self.protocol,
            local: self.local,
            remote: self.remote,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SocketTrafficCounters {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TrafficRate {
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub sample_window_ms: u64,
}

impl TrafficRate {
    pub fn total_bytes_per_sec(self) -> f64 {
        self.rx_bytes_per_sec + self.tx_bytes_per_sec
    }
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
    /// Newly opened sockets per second over the latest poll interval.
    pub hits_per_sec: f64,
    pub current_connections: usize,
    pub traffic: Option<TrafficRate>,
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
                || v4.octets()[0] == 100 && (v4.octets()[1] & 0b1100_0000) == 0b0100_0000
            // CGNAT 100.64/10
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
        assert!(is_local_or_private(IpAddr::V4(Ipv4Addr::new(
            192, 168, 1, 1
        ))));
        assert!(is_local_or_private(IpAddr::V4(Ipv4Addr::new(
            172, 16, 0, 1
        ))));
        assert!(is_local_or_private(IpAddr::V4(Ipv4Addr::new(
            100, 64, 0, 1
        )))); // CGNAT
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
