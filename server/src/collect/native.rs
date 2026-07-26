use std::net::SocketAddr;

use crate::model::{ConnState, Protocol, SocketKey};

#[derive(Debug, Clone)]
pub struct RawSocket {
    pub protocol: Protocol,
    pub local: SocketAddr,
    pub remote: SocketAddr,
    pub state: ConnState,
    pub pids: Vec<u32>,
    pub native_id: u64,
    pub counters: Option<SocketCounters>,
}

/// Cumulative byte counters returned with a native socket observation.
#[derive(Debug, Clone, Copy, Default)]
pub struct SocketCounters {
    pub tx_bytes: u64,
    pub rx_bytes: u64,
}

impl RawSocket {
    pub fn key(&self) -> SocketKey {
        SocketKey {
            protocol: self.protocol,
            local: self.local,
            remote: self.remote,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NativeSnapshot {
    pub sockets: Vec<RawSocket>,
    pub source: &'static str,
    pub udp_remote: bool,
    pub access_limited: usize,
    pub truncated_sockets: usize,
    pub warnings: Vec<String>,
    pub traffic_counters: bool,
}

#[cfg(target_os = "linux")]
mod platform {
    use std::collections::{HashMap, HashSet};
    use std::fs::{read_dir, read_link};
    use std::io;
    use std::net::SocketAddr;

    use netlink_packet_core::{
        NetlinkHeader, NetlinkMessage, NetlinkPayload, NLM_F_DUMP, NLM_F_REQUEST,
    };
    use netlink_packet_sock_diag::{
        constants::{AF_INET, AF_INET6, IPPROTO_TCP, IPPROTO_UDP},
        inet::{nlas::Nla, ExtensionFlags, InetRequest, SocketId, StateFlags},
        SockDiagMessage,
    };
    use netlink_sys::{protocols::NETLINK_SOCK_DIAG, Socket, SocketAddr as NetlinkAddr};

    use super::SocketCounters;
    use super::{NativeSnapshot, RawSocket};
    use crate::model::{ConnState, Protocol};

    pub fn snapshot(include_udp: bool, enhanced: bool) -> io::Result<NativeSnapshot> {
        // Bracket the kernel dump with ownership scans. This catches both a
        // socket that exits immediately after the dump and one created while
        // the first /proc walk is in progress.
        let (before, denied_before) = scan_inode_owners();
        let mut sockets = Vec::new();
        let diag_result = (|| {
            dump_family(
                AF_INET,
                IPPROTO_TCP,
                Protocol::Tcp,
                SocketId::new_v4(),
                enhanced,
                &mut sockets,
            )?;
            dump_family(
                AF_INET6,
                IPPROTO_TCP,
                Protocol::Tcp,
                SocketId::new_v6(),
                enhanced,
                &mut sockets,
            )?;
            if include_udp {
                dump_family(
                    AF_INET,
                    IPPROTO_UDP,
                    Protocol::Udp,
                    SocketId::new_v4(),
                    false,
                    &mut sockets,
                )?;
                dump_family(
                    AF_INET6,
                    IPPROTO_UDP,
                    Protocol::Udp,
                    SocketId::new_v6(),
                    false,
                    &mut sockets,
                )?;
            }
            Ok::<(), io::Error>(())
        })();
        let (source, warnings, traffic_counters) = match diag_result {
            Ok(()) => ("linux-sock-diag", Vec::new(), enhanced),
            Err(error) => {
                sockets.clear();
                procfs_sockets(include_udp, &mut sockets)?;
                (
                    "linux-procfs",
                    vec![format!("sock_diag unavailable; using procfs ({error})")],
                    false,
                )
            }
        };
        let unresolved = sockets
            .iter()
            .any(|socket| !before.contains_key(&(socket.native_id as u32)));
        let (after, denied_after) = if unresolved {
            scan_inode_owners()
        } else {
            (HashMap::new(), 0)
        };
        for socket in &mut sockets {
            let inode = socket.native_id as u32;
            let mut owners = before.get(&inode).cloned().unwrap_or_default();
            if let Some(later) = after.get(&inode) {
                owners.extend(later);
            }
            socket.pids = owners.into_iter().collect();
            socket.pids.sort_unstable();
        }

        Ok(NativeSnapshot {
            sockets,
            source,
            udp_remote: true,
            access_limited: denied_before.max(denied_after),
            truncated_sockets: 0,
            warnings,
            traffic_counters,
        })
    }

    fn procfs_sockets(include_udp: bool, out: &mut Vec<RawSocket>) -> io::Result<()> {
        parse_proc_table("/proc/net/tcp", Protocol::Tcp, false, out)?;
        parse_proc_table("/proc/net/tcp6", Protocol::Tcp, true, out)?;
        if include_udp {
            parse_proc_table("/proc/net/udp", Protocol::Udp, false, out)?;
            parse_proc_table("/proc/net/udp6", Protocol::Udp, true, out)?;
        }
        Ok(())
    }

    fn parse_proc_table(
        path: &str,
        protocol: Protocol,
        ipv6: bool,
        out: &mut Vec<RawSocket>,
    ) -> io::Result<()> {
        let contents = std::fs::read_to_string(path)?;
        for line in contents.lines().skip(1) {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 10 {
                continue;
            }
            let Some(local) = parse_proc_address(fields[1], ipv6) else {
                continue;
            };
            let Some(remote) = parse_proc_address(fields[2], ipv6) else {
                continue;
            };
            if remote.port() == 0 || remote.ip().is_unspecified() {
                continue;
            }
            let state_number = u8::from_str_radix(fields[3], 16).unwrap_or_default();
            let state = state_from_linux(state_number, protocol);
            if matches!(state, ConnState::Listen) {
                continue;
            }
            let inode = fields
                .get(9)
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or_default();
            out.push(RawSocket {
                protocol,
                local,
                remote,
                state,
                pids: Vec::new(),
                native_id: inode,
                counters: None,
            });
        }
        Ok(())
    }

    fn parse_proc_address(value: &str, ipv6: bool) -> Option<SocketAddr> {
        let (address, port) = value.split_once(':')?;
        let port = u16::from_str_radix(port, 16).ok()?;
        let ip = if ipv6 {
            if address.len() != 32 {
                return None;
            }
            let mut bytes = [0u8; 16];
            for word in 0..4 {
                let start = word * 8;
                let value = u32::from_str_radix(&address[start..start + 8], 16).ok()?;
                bytes[start / 2..start / 2 + 4].copy_from_slice(&value.to_le_bytes());
            }
            std::net::IpAddr::V6(std::net::Ipv6Addr::from(bytes))
        } else {
            let value = u32::from_str_radix(address, 16).ok()?;
            std::net::IpAddr::V4(std::net::Ipv4Addr::from(value.to_le_bytes()))
        };
        Some(SocketAddr::new(ip, port))
    }

    fn scan_inode_owners() -> (HashMap<u32, HashSet<u32>>, usize) {
        let mut owners = HashMap::<u32, HashSet<u32>>::new();
        let mut denied = 0;
        let Ok(processes) = read_dir("/proc") else {
            return (owners, 1);
        };
        for process in processes.flatten() {
            let Some(pid) = process
                .file_name()
                .to_str()
                .and_then(|value| value.parse().ok())
            else {
                continue;
            };
            let fds = match read_dir(process.path().join("fd")) {
                Ok(fds) => fds,
                Err(error) => {
                    if error.kind() == io::ErrorKind::PermissionDenied {
                        denied += 1;
                    }
                    continue;
                }
            };
            for fd in fds.flatten() {
                let Ok(link) = read_link(fd.path()) else {
                    continue;
                };
                let value = link.to_string_lossy();
                let Some(inode) = value
                    .strip_prefix("socket:[")
                    .and_then(|value| value.strip_suffix(']'))
                    .and_then(|value| value.parse::<u32>().ok())
                else {
                    continue;
                };
                owners.entry(inode).or_default().insert(pid);
            }
        }
        (owners, denied)
    }

    fn dump_family(
        family: u8,
        ip_protocol: u8,
        protocol: Protocol,
        socket_id: SocketId,
        enhanced: bool,
        out: &mut Vec<RawSocket>,
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
                protocol: ip_protocol,
                extensions: if enhanced && matches!(protocol, Protocol::Tcp) {
                    ExtensionFlags::INFO
                } else {
                    ExtensionFlags::empty()
                },
                states: StateFlags::all(),
                socket_id,
            })
            .into(),
        );
        request.finalize();
        let mut send = vec![0; request.buffer_len()];
        request.serialize(&mut send);
        socket.send(&send, 0)?;

        let mut receive = vec![0u8; 64 * 1024];
        loop {
            let size = socket.recv(&mut &mut receive[..], 0)?;
            let mut offset = 0usize;
            while offset < size {
                let packet: NetlinkMessage<SockDiagMessage> =
                    NetlinkMessage::deserialize(&receive[offset..size]).map_err(|error| {
                        io::Error::new(io::ErrorKind::InvalidData, error.to_string())
                    })?;
                let length = packet.header.length as usize;
                if length == 0 || offset + length > size {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid socket diagnostic packet length",
                    ));
                }
                offset += (length + 3) & !3;
                match packet.payload {
                    NetlinkPayload::InnerMessage(SockDiagMessage::InetResponse(response)) => {
                        let id = &response.header.socket_id;
                        if id.destination_port == 0 || id.destination_address.is_unspecified() {
                            continue;
                        }
                        let state = state_from_linux(response.header.state, protocol);
                        if matches!(state, ConnState::Listen) {
                            continue;
                        }
                        let counters = response.nlas.iter().find_map(|nla| match nla {
                            Nla::TcpInfo(info) => Some(SocketCounters {
                                tx_bytes: info.bytes_acked,
                                rx_bytes: info.bytes_received,
                            }),
                            _ => None,
                        });
                        out.push(RawSocket {
                            protocol,
                            local: SocketAddr::new(id.source_address, id.source_port),
                            remote: SocketAddr::new(id.destination_address, id.destination_port),
                            state,
                            pids: Vec::new(),
                            native_id: response.header.inode as u64,
                            counters,
                        });
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

    fn state_from_linux(state: u8, protocol: Protocol) -> ConnState {
        if matches!(protocol, Protocol::Udp) {
            return ConnState::Established;
        }
        match state {
            1 => ConnState::Established,
            2 | 3 => ConnState::Connecting,
            4 | 5 | 8 | 9 | 11 => ConnState::Closing,
            6 => ConnState::TimeWait,
            7 => ConnState::Closed,
            10 => ConnState::Listen,
            _ => ConnState::Unknown,
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::io;

    use super::NativeSnapshot;

    pub fn snapshot(include_udp: bool, _enhanced: bool) -> io::Result<NativeSnapshot> {
        let scan = crate::collect::udp::snapshot(include_udp)?;
        let mut warnings = Vec::new();
        if scan.truncated_sockets > 0 {
            warnings.push(format!(
                "{} socket records exceeded the collector safety limit",
                scan.truncated_sockets
            ));
        }
        let _transient_processes = scan.transient_processes;
        Ok(NativeSnapshot {
            sockets: scan.sockets,
            source: "macos-libproc",
            udp_remote: include_udp,
            access_limited: scan.inaccessible_processes,
            truncated_sockets: scan.truncated_sockets,
            warnings,
            traffic_counters: false,
        })
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::io;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, MIB_TCP_STATE_CLOSED,
        MIB_TCP_STATE_DELETE_TCB, MIB_TCP_STATE_ESTAB, MIB_TCP_STATE_LISTEN,
        MIB_TCP_STATE_SYN_RCVD, MIB_TCP_STATE_SYN_SENT, MIB_TCP_STATE_TIME_WAIT,
        TCP_TABLE_OWNER_PID_ALL,
    };
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};

    use super::{NativeSnapshot, RawSocket};
    use crate::model::{ConnState, Protocol};

    pub fn snapshot(include_udp: bool, _enhanced: bool) -> io::Result<NativeSnapshot> {
        let mut sockets = tcp_v4()?;
        sockets.extend(tcp_v6()?);
        let mut warnings = Vec::new();
        let mut access_limited = 0;
        let mut udp_remote = !include_udp;
        if include_udp {
            match crate::collect::udp::snapshot() {
                Ok(udp) => {
                    access_limited = udp.skipped_processes;
                    udp_remote = true;
                    sockets.extend(udp.observations.into_iter().map(|socket| RawSocket {
                        protocol: Protocol::Udp,
                        local: socket.local,
                        remote: socket.remote,
                        state: ConnState::Established,
                        pids: socket.pids,
                        native_id: 0,
                        counters: None,
                    }));
                }
                Err(error) => {
                    warnings.push(format!("Connected UDP collection is unavailable: {error}"))
                }
            }
        }
        Ok(NativeSnapshot {
            sockets,
            source: "windows-ip-helper",
            udp_remote,
            access_limited,
            truncated_sockets: 0,
            warnings,
            traffic_counters: false,
        })
    }

    fn table(family: u32) -> io::Result<Vec<u32>> {
        let mut bytes = 0u32;
        let first = unsafe {
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut bytes,
                0,
                family,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };
        if first != ERROR_INSUFFICIENT_BUFFER && first != NO_ERROR {
            return Err(io::Error::from_raw_os_error(first as i32));
        }
        let mut buffer = vec![0u32; (bytes as usize).div_ceil(size_of::<u32>())];
        let result = unsafe {
            GetExtendedTcpTable(
                buffer.as_mut_ptr().cast(),
                &mut bytes,
                0,
                family,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };
        if result == NO_ERROR {
            Ok(buffer)
        } else {
            Err(io::Error::from_raw_os_error(result as i32))
        }
    }

    use std::mem::size_of;

    fn tcp_v4() -> io::Result<Vec<RawSocket>> {
        let buffer = table(AF_INET as u32)?;
        let count = buffer.first().copied().unwrap_or_default() as usize;
        let rows = unsafe {
            std::slice::from_raw_parts(buffer.as_ptr().add(1).cast::<MIB_TCPROW_OWNER_PID>(), count)
        };
        Ok(rows
            .iter()
            .filter_map(|row| {
                raw_tcp(
                    IpAddr::V4(Ipv4Addr::from(u32::from_be(row.dwLocalAddr))),
                    row.dwLocalPort,
                    IpAddr::V4(Ipv4Addr::from(u32::from_be(row.dwRemoteAddr))),
                    row.dwRemotePort,
                    row.dwState,
                    row.dwOwningPid,
                )
            })
            .collect())
    }

    fn tcp_v6() -> io::Result<Vec<RawSocket>> {
        let buffer = table(AF_INET6 as u32)?;
        let count = buffer.first().copied().unwrap_or_default() as usize;
        let rows = unsafe {
            std::slice::from_raw_parts(
                buffer.as_ptr().add(1).cast::<MIB_TCP6ROW_OWNER_PID>(),
                count,
            )
        };
        Ok(rows
            .iter()
            .filter_map(|row| {
                raw_tcp(
                    IpAddr::V6(Ipv6Addr::from(row.ucLocalAddr)),
                    row.dwLocalPort,
                    IpAddr::V6(Ipv6Addr::from(row.ucRemoteAddr)),
                    row.dwRemotePort,
                    row.dwState,
                    row.dwOwningPid,
                )
            })
            .collect())
    }

    fn raw_tcp(
        local_ip: IpAddr,
        local_port: u32,
        remote_ip: IpAddr,
        remote_port: u32,
        state: u32,
        pid: u32,
    ) -> Option<RawSocket> {
        let state = match state as i32 {
            MIB_TCP_STATE_SYN_SENT | MIB_TCP_STATE_SYN_RCVD => ConnState::Connecting,
            MIB_TCP_STATE_ESTAB => ConnState::Established,
            MIB_TCP_STATE_TIME_WAIT => ConnState::TimeWait,
            MIB_TCP_STATE_LISTEN => ConnState::Listen,
            MIB_TCP_STATE_CLOSED | MIB_TCP_STATE_DELETE_TCB => ConnState::Closed,
            _ => ConnState::Closing,
        };
        let remote_port = u16::from_be(remote_port as u16);
        if matches!(state, ConnState::Listen) || remote_port == 0 || remote_ip.is_unspecified() {
            return None;
        }
        Some(RawSocket {
            protocol: Protocol::Tcp,
            local: SocketAddr::new(local_ip, u16::from_be(local_port as u16)),
            remote: SocketAddr::new(remote_ip, remote_port),
            state,
            pids: vec![pid],
            native_id: 0,
            counters: None,
        })
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    use std::io;

    use super::NativeSnapshot;

    pub fn snapshot(_include_udp: bool, _enhanced: bool) -> io::Result<NativeSnapshot> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native socket collection is unsupported on this platform",
        ))
    }
}

pub fn snapshot(include_udp: bool, enhanced: bool) -> std::io::Result<NativeSnapshot> {
    platform::snapshot(include_udp, enhanced)
}

#[cfg(test)]
mod tests {
    use std::net::UdpSocket;
    use std::time::{Duration, Instant};

    fn udp_snapshot() -> super::NativeSnapshot {
        let started = Instant::now();
        loop {
            match super::snapshot(true, false) {
                Ok(snapshot) if snapshot.udp_remote => return snapshot,
                Ok(_) | Err(_) if started.elapsed() < Duration::from_secs(10) => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Ok(snapshot) => return snapshot,
                Err(error) => panic!("connected UDP collector did not become ready: {error}"),
            }
        }
    }

    #[test]
    #[cfg_attr(
        target_os = "linux",
        ignore = "requires Linux socket-diagnostic access"
    )]
    fn connected_udp_socket_is_reported_for_current_process() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender.connect(receiver.local_addr().unwrap()).unwrap();

        let snapshot = udp_snapshot();
        let pid = std::process::id();
        assert!(snapshot.sockets.iter().any(|socket| {
            socket.protocol == crate::model::Protocol::Udp
                && socket.local == sender.local_addr().unwrap()
                && socket.remote == receiver.local_addr().unwrap()
                && socket.pids.contains(&pid)
        }));
    }

    #[test]
    #[cfg_attr(
        target_os = "linux",
        ignore = "requires Linux socket-diagnostic access"
    )]
    fn unconnected_udp_socket_is_not_reported_as_a_peer() {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let local = socket.local_addr().unwrap();
        let snapshot = udp_snapshot();
        assert!(!snapshot.sockets.iter().any(|candidate| {
            candidate.protocol == crate::model::Protocol::Udp && candidate.local == local
        }));
    }
}
