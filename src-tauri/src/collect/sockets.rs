use anyhow::{Context, Result};
use netstat2::{
    get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState,
};

use crate::model::{ConnState, Connection, Protocol};

use super::process;

pub fn snapshot(include_udp: bool) -> Result<Vec<Connection>> {
    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let mut proto = ProtocolFlags::TCP;
    if include_udp {
        proto |= ProtocolFlags::UDP;
    }

    let sockets = get_sockets_info(af, proto).context("failed to read system socket table")?;
    let mut out = Vec::with_capacity(sockets.len());

    for si in sockets {
        let (protocol, local, remote, state) = match si.protocol_socket_info {
            ProtocolSocketInfo::Tcp(tcp) => {
                let state = match tcp.state {
                    TcpState::Established => ConnState::Established,
                    TcpState::Listen => ConnState::Listen,
                    _ => ConnState::Other,
                };
                // Focus on connections with a remote peer (active traffic)
                if matches!(state, ConnState::Listen) {
                    continue;
                }
                if !matches!(state, ConnState::Established | ConnState::Other) {
                    continue;
                }
                // Prefer established; still keep TimeWait/etc lightly via Other if remote set
                if matches!(state, ConnState::Other) && tcp.remote_addr.is_unspecified() {
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

        // Associated processes: netstat2 may return multiple PIDs for a socket
        let pids = si.associated_pids;
        if pids.is_empty() {
            out.push(Connection {
                pid: None,
                process_name: "unknown".into(),
                process_path: None,
                local,
                remote,
                protocol,
                state,
            });
        } else {
            for pid in pids {
                let (name, path) = process::resolve(pid);
                out.push(Connection {
                    pid: Some(pid),
                    process_name: name,
                    process_path: path,
                    local,
                    remote,
                    protocol,
                    state,
                });
            }
        }
    }

    Ok(out)
}
