use std::collections::HashMap;

use crate::model::SocketKey;

#[derive(Debug, Clone, Copy, Default)]
pub struct SocketCounters {
    pub tx_bytes: u64,
    pub rx_bytes: u64,
}

#[cfg(target_os = "linux")]
mod platform {
    use std::io;
    use std::net::SocketAddr;

    use netlink_packet_core::{
        NetlinkHeader, NetlinkMessage, NetlinkPayload, NLM_F_DUMP, NLM_F_REQUEST,
    };
    use netlink_packet_sock_diag::{
        constants::{AF_INET, AF_INET6, IPPROTO_TCP},
        inet::{nlas::Nla, ExtensionFlags, InetRequest, SocketId, StateFlags},
        SockDiagMessage,
    };
    use netlink_sys::{protocols::NETLINK_SOCK_DIAG, Socket, SocketAddr as NetlinkAddr};

    use super::{HashMap, SocketCounters, SocketKey};
    use crate::model::Protocol;

    pub fn snapshot() -> io::Result<HashMap<SocketKey, SocketCounters>> {
        let mut counters = HashMap::new();
        dump_family(AF_INET as u8, SocketId::new_v4(), &mut counters)?;
        dump_family(AF_INET6 as u8, SocketId::new_v6(), &mut counters)?;
        Ok(counters)
    }

    fn dump_family(
        family: u8,
        socket_id: SocketId,
        counters: &mut HashMap<SocketKey, SocketCounters>,
    ) -> io::Result<()> {
        let mut socket = Socket::new(NETLINK_SOCK_DIAG)?;
        socket.bind_auto()?;
        socket.connect(&NetlinkAddr::new(0, 0))?;

        let mut header = NetlinkHeader::default();
        header.flags = NLM_F_REQUEST | NLM_F_DUMP;
        let mut request = NetlinkMessage::new(
            header,
            SockDiagMessage::InetRequest(InetRequest {
                family,
                protocol: IPPROTO_TCP as u8,
                extensions: ExtensionFlags::INFO,
                states: StateFlags::all(),
                socket_id,
            })
            .into(),
        );
        request.finalize();
        let mut send_buffer = vec![0; request.buffer_len()];
        request.serialize(&mut send_buffer);
        socket.send(&send_buffer, 0)?;

        let mut receive_buffer = vec![0u8; 64 * 1024];
        loop {
            let size = socket.recv(&mut &mut receive_buffer[..], 0)?;
            let mut offset = 0usize;
            while offset < size {
                let packet: NetlinkMessage<SockDiagMessage> = NetlinkMessage::deserialize(
                    &receive_buffer[offset..size],
                )
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
                let length = packet.header.length as usize;
                if length == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "zero-length socket diagnostic packet",
                    ));
                }
                offset += length;
                match packet.payload {
                    NetlinkPayload::InnerMessage(SockDiagMessage::InetResponse(response)) => {
                        let Some(info) = response.nlas.iter().find_map(|nla| match nla {
                            Nla::TcpInfo(info) => Some(info),
                            _ => None,
                        }) else {
                            continue;
                        };
                        let id = &response.header.socket_id;
                        if id.destination_port == 0 || id.destination_address.is_unspecified() {
                            continue;
                        }
                        counters.insert(
                            SocketKey {
                                protocol: Protocol::Tcp,
                                local: SocketAddr::new(id.source_address, id.source_port),
                                remote: SocketAddr::new(
                                    id.destination_address,
                                    id.destination_port,
                                ),
                            },
                            SocketCounters {
                                tx_bytes: info.bytes_acked,
                                rx_bytes: info.bytes_received,
                            },
                        );
                    }
                    NetlinkPayload::Done(_) => return Ok(()),
                    NetlinkPayload::Error(error) => {
                        return Err(io::Error::other(format!(
                            "socket diagnostic error: {error:?}"
                        )));
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use std::io;

    use super::{HashMap, SocketCounters, SocketKey};

    pub fn snapshot() -> io::Result<HashMap<SocketKey, SocketCounters>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native byte counters are not implemented on this platform",
        ))
    }
}

pub use platform::snapshot;

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    #[ignore = "requires Linux socket-diagnostic access"]
    fn reads_live_tcp_counters() {
        let counters = super::snapshot().expect("socket diagnostics should be readable");
        assert!(
            counters.keys().all(|socket| socket.remote.port() != 0),
            "only connected TCP sockets should be returned"
        );
    }
}
