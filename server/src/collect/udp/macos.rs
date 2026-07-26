use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::collect::native::RawSocket;
use crate::model::{ConnState, Protocol};

const AF_INET: u8 = 2;
const AF_INET6: u8 = 30;
const IPPROTO_TCP: u8 = 6;
const IPPROTO_UDP: u8 = 17;
const INITIAL_SOCKET_CAPACITY: usize = 8_192;
const MAX_SOCKET_CAPACITY: usize = 262_144;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NativeSocketPeer {
    pid: u32,
    protocol: u8,
    family: u8,
    state: u8,
    reserved: u8,
    local_port: u16,
    remote_port: u16,
    local_addr: [u8; 16],
    remote_addr: [u8; 16],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NativeScanStats {
    matched: usize,
    written: usize,
    inaccessible_processes: u32,
    transient_processes: u32,
}

unsafe extern "C" {
    fn nc_collect_sockets(
        include_udp: i32,
        output: *mut NativeSocketPeer,
        capacity: usize,
        stats: *mut NativeScanStats,
    ) -> i32;
}

#[derive(Debug)]
pub struct MacSocketSnapshot {
    pub sockets: Vec<RawSocket>,
    pub inaccessible_processes: usize,
    pub transient_processes: usize,
    pub truncated_sockets: usize,
}

pub fn snapshot(include_udp: bool) -> io::Result<MacSocketSnapshot> {
    let mut capacity = INITIAL_SOCKET_CAPACITY;
    loop {
        let mut peers = vec![NativeSocketPeer::default(); capacity];
        let mut stats = NativeScanStats::default();
        let result = unsafe {
            nc_collect_sockets(
                if include_udp { 1 } else { 0 },
                peers.as_mut_ptr(),
                peers.len(),
                &mut stats,
            )
        };
        if result != 0 {
            return Err(io::Error::from_raw_os_error(result));
        }

        if stats.matched > capacity && capacity < MAX_SOCKET_CAPACITY {
            capacity = stats.matched.next_power_of_two().min(MAX_SOCKET_CAPACITY);
            continue;
        }

        peers.truncate(stats.written.min(peers.len()));
        let sockets = peers.into_iter().filter_map(raw_socket).collect();
        return Ok(MacSocketSnapshot {
            sockets,
            inaccessible_processes: stats.inaccessible_processes as usize,
            transient_processes: stats.transient_processes as usize,
            truncated_sockets: stats.matched.saturating_sub(stats.written),
        });
    }
}

fn raw_socket(peer: NativeSocketPeer) -> Option<RawSocket> {
    let (local, remote) = addresses(peer.family, peer.local_addr, peer.remote_addr)?;
    let protocol = match peer.protocol {
        IPPROTO_TCP => Protocol::Tcp,
        IPPROTO_UDP => Protocol::Udp,
        _ => return None,
    };
    Some(RawSocket {
        protocol,
        local: SocketAddr::new(local, peer.local_port),
        remote: SocketAddr::new(remote, peer.remote_port),
        state: if protocol == Protocol::Udp {
            ConnState::Established
        } else {
            match peer.state {
                1 => ConnState::Listen,
                2 | 3 => ConnState::Connecting,
                4 => ConnState::Established,
                10 => ConnState::TimeWait,
                0 => ConnState::Closed,
                _ => ConnState::Closing,
            }
        },
        pids: vec![peer.pid],
        native_id: 0,
        counters: None,
    })
}

fn addresses(family: u8, local: [u8; 16], remote: [u8; 16]) -> Option<(IpAddr, IpAddr)> {
    match family {
        AF_INET => Some((
            IpAddr::V4(Ipv4Addr::new(local[0], local[1], local[2], local[3])),
            IpAddr::V4(Ipv4Addr::new(remote[0], remote[1], remote[2], remote[3])),
        )),
        AF_INET6 => Some((
            IpAddr::V6(Ipv6Addr::from(local)),
            IpAddr::V6(Ipv6Addr::from(remote)),
        )),
        _ => None,
    }
}
