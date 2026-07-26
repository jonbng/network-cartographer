use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use super::{UdpObservation, UdpSnapshot};
use crate::collect::native::RawSocket;
use crate::model::{ConnState, Protocol};

const AF_INET: u8 = 2;
const AF_INET6: u8 = 30;
const MAX_SOCKET_PEERS: usize = 8192;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NativeUdpPeer {
    pid: u32,
    family: u8,
    reserved: [u8; 3],
    local_port: u16,
    remote_port: u16,
    local_addr: [u8; 16],
    remote_addr: [u8; 16],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NativeTcpPeer {
    pid: u32,
    family: u8,
    state: u8,
    reserved: [u8; 2],
    local_port: u16,
    remote_port: u16,
    local_addr: [u8; 16],
    remote_addr: [u8; 16],
}

unsafe extern "C" {
    fn nc_collect_udp(output: *mut NativeUdpPeer, capacity: usize, written: *mut usize) -> i32;
    fn nc_collect_tcp(output: *mut NativeTcpPeer, capacity: usize, written: *mut usize) -> i32;
}

pub fn tcp_snapshot() -> io::Result<Vec<RawSocket>> {
    let mut peers = vec![NativeTcpPeer::default(); MAX_SOCKET_PEERS];
    let mut written = 0usize;
    let result = unsafe { nc_collect_tcp(peers.as_mut_ptr(), peers.len(), &mut written) };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result));
    }
    peers.truncate(written.min(peers.len()));
    Ok(peers
        .into_iter()
        .filter_map(|peer| {
            let (local, remote) = addresses(peer.family, peer.local_addr, peer.remote_addr)?;
            Some(RawSocket {
                protocol: Protocol::Tcp,
                local: SocketAddr::new(local, peer.local_port),
                remote: SocketAddr::new(remote, peer.remote_port),
                state: match peer.state {
                    1 => ConnState::Listen,
                    2 | 3 => ConnState::Connecting,
                    4 => ConnState::Established,
                    10 => ConnState::TimeWait,
                    0 => ConnState::Closed,
                    _ => ConnState::Closing,
                },
                pids: vec![peer.pid],
                native_id: 0,
                counters: None,
            })
        })
        .collect())
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

pub fn snapshot() -> io::Result<UdpSnapshot> {
    let mut peers = vec![NativeUdpPeer::default(); MAX_SOCKET_PEERS];
    let mut written = 0usize;
    let result = unsafe { nc_collect_udp(peers.as_mut_ptr(), peers.len(), &mut written) };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result));
    }
    peers.truncate(written.min(peers.len()));

    let observations = peers
        .into_iter()
        .filter_map(|peer| {
            let (local_ip, remote_ip) = addresses(peer.family, peer.local_addr, peer.remote_addr)?;
            Some(UdpObservation {
                local: SocketAddr::new(local_ip, peer.local_port),
                remote: SocketAddr::new(remote_ip, peer.remote_port),
                pids: vec![peer.pid],
            })
        })
        .collect();

    Ok(UdpSnapshot {
        observations,
        skipped_processes: 0,
    })
}
