use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState};

use crate::model::{
    AttributionSource, ConnState, Connection, Protocol, SocketKey, UnattributedReason,
};

use super::process;
use super::traffic;

#[derive(Debug, Clone, Default)]
pub enum NativeTrafficStatus {
    #[default]
    Disabled,
    Available,
    Unavailable(String),
}

#[derive(Debug, Clone)]
struct Owner {
    pid: u32,
    name: String,
    path: Option<std::path::PathBuf>,
}

impl Owner {
    fn identity(&self) -> String {
        self.path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| {
                if self.name.is_empty() || self.name == "unknown" {
                    format!("pid:{}", self.pid)
                } else {
                    self.name.clone()
                }
            })
    }
}

#[derive(Debug, Default)]
pub struct SocketCollector {
    observed: HashMap<SocketKey, Option<Owner>>,
    traffic_status: NativeTrafficStatus,
}

impl SocketCollector {
    fn attribute(
        &mut self,
        key: &SocketKey,
        mut owners: Vec<Owner>,
    ) -> (
        Option<Owner>,
        AttributionSource,
        Option<UnattributedReason>,
        bool,
    ) {
        let is_new = !self.observed.contains_key(key);
        owners.sort_by_key(Owner::identity);
        owners.dedup_by(|a, b| a.identity() == b.identity());
        let result = match owners.len() {
            1 => (owners.pop(), AttributionSource::Direct, None, is_new),
            0 => match self.observed.get(key).and_then(Clone::clone) {
                Some(owner) => (Some(owner), AttributionSource::Recovered, None, is_new),
                None => (
                    None,
                    AttributionSource::Unattributed,
                    Some(UnattributedReason::OwnerGone),
                    is_new,
                ),
            },
            _ => (
                None,
                AttributionSource::Unattributed,
                Some(UnattributedReason::Ambiguous),
                is_new,
            ),
        };
        self.observed.insert(key.clone(), result.0.clone());
        result
    }

    pub fn snapshot(&mut self, include_udp: bool, enhanced: bool) -> Result<Vec<Connection>> {
        let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
        let mut proto = ProtocolFlags::TCP;
        if include_udp {
            proto |= ProtocolFlags::UDP;
        }

        let traffic = if enhanced {
            match traffic::snapshot() {
                Ok(counters) => {
                    self.traffic_status = NativeTrafficStatus::Available;
                    counters
                }
                Err(error) => {
                    self.traffic_status = NativeTrafficStatus::Unavailable(error.to_string());
                    HashMap::new()
                }
            }
        } else {
            self.traffic_status = NativeTrafficStatus::Disabled;
            HashMap::new()
        };
        let sockets = get_sockets_info(af, proto).context("failed to read system socket table")?;
        let mut out = Vec::with_capacity(sockets.len());

        for si in sockets {
            let (protocol, local, remote, state) = match si.protocol_socket_info {
                ProtocolSocketInfo::Tcp(tcp) => {
                    let state = match tcp.state {
                        TcpState::SynSent | TcpState::SynReceived => ConnState::Connecting,
                        TcpState::Established => ConnState::Established,
                        TcpState::FinWait1
                        | TcpState::FinWait2
                        | TcpState::CloseWait
                        | TcpState::Closing
                        | TcpState::LastAck => ConnState::Closing,
                        TcpState::TimeWait => ConnState::TimeWait,
                        TcpState::Listen => ConnState::Listen,
                        TcpState::Closed | TcpState::DeleteTcb => ConnState::Closed,
                        TcpState::Unknown => ConnState::Unknown,
                    };
                    // Focus on connections with a remote peer (active traffic)
                    if matches!(state, ConnState::Listen) {
                        continue;
                    }
                    if tcp.remote_addr.is_unspecified() || tcp.remote_port == 0 {
                        continue;
                    }
                    (
                        Protocol::Tcp,
                        std::net::SocketAddr::new(tcp.local_addr, tcp.local_port),
                        std::net::SocketAddr::new(tcp.remote_addr, tcp.remote_port),
                        state,
                    )
                }
                ProtocolSocketInfo::Udp(udp) => {
                    // netstat2 only exposes local bind info for UDP (no remote peer).
                    // Skip for now — outbound UDP destinations need OS-specific APIs.
                    let _ = udp;
                    if !include_udp {
                        continue;
                    }
                    continue;
                }
            };

            let key = SocketKey {
                protocol,
                local,
                remote,
            };
            let owners: Vec<Owner> = si
                .associated_pids
                .into_iter()
                .map(|pid| {
                    let (name, path) = process::resolve(pid);
                    Owner { pid, name, path }
                })
                .collect();
            let (owner, attribution, reason, is_new) = self.attribute(&key, owners);
            out.push(Connection {
                pid: owner.as_ref().map(|owner| owner.pid),
                process_name: owner
                    .as_ref()
                    .map(|owner| owner.name.clone())
                    .unwrap_or_default(),
                process_path: owner.as_ref().and_then(|owner| owner.path.clone()),
                local,
                remote,
                protocol,
                state,
                attribution,
                unattributed_reason: reason,
                is_new,
                traffic_counters: traffic.get(&key).map(|counters| {
                    crate::model::SocketTrafficCounters {
                        rx_bytes: counters.rx_bytes,
                        tx_bytes: counters.tx_bytes,
                    }
                }),
            });
        }

        let present: HashSet<_> = out.iter().map(Connection::socket_key).collect();
        self.observed.retain(|key, _| present.contains(key));

        Ok(out)
    }

    pub fn reset(&mut self) {
        self.observed.clear();
    }

    pub fn traffic_status(&self) -> NativeTrafficStatus {
        self.traffic_status.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(port: u16) -> SocketKey {
        SocketKey {
            protocol: Protocol::Tcp,
            local: format!("127.0.0.1:{port}").parse().unwrap(),
            remote: "203.0.113.1:443".parse().unwrap(),
        }
    }

    fn owner(pid: u32, path: &str) -> Owner {
        Owner {
            pid,
            name: "browser".into(),
            path: Some(path.into()),
        }
    }

    #[test]
    fn exact_socket_recovers_owner_during_teardown() {
        let mut collector = SocketCollector::default();
        let socket = key(41000);
        let (_, source, _, is_new) = collector.attribute(&socket, vec![owner(10, "/bin/app")]);
        assert_eq!(source, AttributionSource::Direct);
        assert!(is_new);

        let (recovered, source, reason, is_new) = collector.attribute(&socket, vec![]);
        assert_eq!(source, AttributionSource::Recovered);
        assert_eq!(recovered.unwrap().pid, 10);
        assert_eq!(reason, None);
        assert!(!is_new);
    }

    #[test]
    fn missing_and_ambiguous_owners_are_not_guessed() {
        let mut collector = SocketCollector::default();
        let (_, source, reason, _) = collector.attribute(&key(41001), vec![]);
        assert_eq!(source, AttributionSource::Unattributed);
        assert_eq!(reason, Some(UnattributedReason::OwnerGone));

        let (_, source, reason, _) =
            collector.attribute(&key(41002), vec![owner(10, "/bin/a"), owner(11, "/bin/b")]);
        assert_eq!(source, AttributionSource::Unattributed);
        assert_eq!(reason, Some(UnattributedReason::Ambiguous));
    }
}
