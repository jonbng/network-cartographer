use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use crate::model::SocketKey;

const EVENT_CAPACITY: usize = 8_192;

#[derive(Debug, Clone)]
pub struct CollectionStatus {
    pub mode: &'static str,
    pub source: &'static str,
    pub captures_opens: bool,
    pub captures_closes: bool,
    pub dropped_events: u64,
    pub status: &'static str,
    pub message: String,
    pub udp_remote: bool,
    pub access_limited: usize,
    pub truncated_sockets: usize,
    pub poll_phase: &'static str,
    pub effective_poll_interval_ms: u64,
    pub observed_opens: u64,
    pub observed_closes: u64,
    pub recovered_owners: u64,
    pub unattributed_owner_gone: u64,
    pub unattributed_ambiguous: u64,
    pub unattributed_access_limited: u64,
}

#[derive(Debug, Default)]
struct InboxState {
    queue: VecDeque<SocketKey>,
    dropped: u64,
    backend: BackendState,
}

#[derive(Debug, Default)]
enum BackendState {
    #[default]
    NotStarted,
    Starting,
    Ready,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct LifecycleEvents {
    inner: Arc<Mutex<InboxState>>,
    started: Arc<AtomicBool>,
}

impl Default for LifecycleEvents {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(InboxState::default())),
            started: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl LifecycleEvents {
    pub fn ensure_started(&self) {
        if self.started.swap(true, Ordering::AcqRel) {
            return;
        }

        #[cfg(target_os = "linux")]
        {
            self.set_backend(BackendState::Starting);
            platform::spawn(Arc::clone(&self.inner));
        }

        #[cfg(not(target_os = "linux"))]
        self.set_backend(BackendState::Ready);
    }

    pub fn drain(&self) -> Vec<SocketKey> {
        let Ok(mut state) = self.inner.lock() else {
            return Vec::new();
        };
        state.queue.drain(..).collect()
    }

    pub fn has_pending(&self) -> bool {
        self.inner
            .lock()
            .map(|state| !state.queue.is_empty())
            .unwrap_or(false)
    }

    pub fn clear(&self) {
        if let Ok(mut state) = self.inner.lock() {
            state.queue.clear();
            state.dropped = 0;
        }
    }

    pub fn status(&self) -> CollectionStatus {
        let state = self.inner.lock().ok();
        let dropped = state.as_ref().map(|state| state.dropped).unwrap_or(0);

        #[cfg(target_os = "linux")]
        {
            let (status, message) = match state.as_ref().map(|state| &state.backend) {
                Some(BackendState::Ready) => (
                    "ready",
                    "Linux TCP close events with portable socket reconciliation".to_string(),
                ),
                Some(BackendState::Failed(error)) => (
                    "degraded",
                    format!("TCP event listener unavailable; using socket polling ({error})"),
                ),
                Some(BackendState::Starting) => (
                    "ready",
                    "Starting Linux TCP close-event listener".to_string(),
                ),
                _ => (
                    "ready",
                    "Portable socket polling; event listener starts after consent".to_string(),
                ),
            };
            CollectionStatus {
                mode: if matches!(
                    state.as_ref().map(|state| &state.backend),
                    Some(BackendState::Ready)
                ) {
                    "event-assisted"
                } else {
                    "adaptive-polling"
                },
                source: if matches!(
                    state.as_ref().map(|state| &state.backend),
                    Some(BackendState::Ready)
                ) {
                    "linux-sock-diag"
                } else {
                    "portable-socket-table"
                },
                captures_opens: false,
                captures_closes: matches!(
                    state.as_ref().map(|state| &state.backend),
                    Some(BackendState::Ready)
                ),
                dropped_events: dropped,
                status,
                message,
                udp_remote: false,
                access_limited: 0,
                truncated_sockets: 0,
                poll_phase: "idle",
                effective_poll_interval_ms: 0,
                observed_opens: 0,
                observed_closes: 0,
                recovered_owners: 0,
                unattributed_owner_gone: 0,
                unattributed_ambiguous: 0,
                unattributed_access_limited: 0,
            }
        }

        #[cfg(not(target_os = "linux"))]
        CollectionStatus {
            mode: "adaptive-polling",
            source: "portable-socket-table",
            captures_opens: false,
            captures_closes: false,
            dropped_events: dropped,
            status: "ready",
            message: "Adaptive TCP socket polling".into(),
            udp_remote: false,
            access_limited: 0,
            truncated_sockets: 0,
            poll_phase: "idle",
            effective_poll_interval_ms: 0,
            observed_opens: 0,
            observed_closes: 0,
            recovered_owners: 0,
            unattributed_owner_gone: 0,
            unattributed_ambiguous: 0,
            unattributed_access_limited: 0,
        }
    }

    fn set_backend(&self, backend: BackendState) {
        if let Ok(mut state) = self.inner.lock() {
            state.backend = backend;
        }
    }
}

fn push(inner: &Mutex<InboxState>, key: SocketKey) {
    if let Ok(mut state) = inner.lock() {
        if state.queue.len() == EVENT_CAPACITY {
            state.queue.pop_front();
            state.dropped = state.dropped.saturating_add(1);
        }
        state.queue.push_back(key);
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::io;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
    use std::thread;

    use netlink_packet_core::{NetlinkMessage, NetlinkPayload};
    use netlink_packet_sock_diag::SockDiagMessage;
    use netlink_sys::{protocols::NETLINK_SOCK_DIAG, Socket, SocketAddr as NetlinkAddr};

    use super::{push, BackendState, InboxState};
    use crate::model::{Protocol, SocketKey};

    // sknetlink_groups values 1 and 3, represented as netlink membership bits.
    const TCP_DESTROY_GROUPS: u32 = 1 | (1 << 2);

    pub fn spawn(inner: Arc<Mutex<InboxState>>) {
        thread::Builder::new()
            .name("tcp-lifecycle".into())
            .spawn(move || match listen(&inner) {
                Ok(()) => set_failed(&inner, "TCP event listener stopped".into()),
                Err(error) => set_failed(&inner, error.to_string()),
            })
            .expect("spawn TCP lifecycle listener");
    }

    fn listen(inner: &Mutex<InboxState>) -> io::Result<()> {
        let mut socket = Socket::new(NETLINK_SOCK_DIAG)?;
        socket.bind(&NetlinkAddr::new(0, TCP_DESTROY_GROUPS))?;
        if let Ok(mut state) = inner.lock() {
            state.backend = BackendState::Ready;
        }

        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let size = socket.recv(&mut &mut buffer[..], 0)?;
            for key in parse_destroy_messages(&buffer[..size])? {
                push(inner, key);
            }
        }
    }

    fn set_failed(inner: &Mutex<InboxState>, error: String) {
        if let Ok(mut state) = inner.lock() {
            state.backend = BackendState::Failed(error);
        }
    }

    fn parse_destroy_messages(bytes: &[u8]) -> io::Result<Vec<SocketKey>> {
        let mut keys = Vec::new();
        let mut offset = 0usize;
        while offset < bytes.len() {
            let packet: NetlinkMessage<SockDiagMessage> =
                NetlinkMessage::deserialize(&bytes[offset..]).map_err(|error| {
                    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
                })?;
            let length = packet.header.length as usize;
            if length == 0 || offset + length > bytes.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid socket diagnostic message length",
                ));
            }
            offset += (length + 3) & !3;

            let NetlinkPayload::InnerMessage(SockDiagMessage::InetResponse(response)) =
                packet.payload
            else {
                continue;
            };
            let id = response.header.socket_id;
            if id.destination_port == 0 || id.destination_address.is_unspecified() {
                continue;
            }
            keys.push(SocketKey {
                protocol: Protocol::Tcp,
                local: SocketAddr::new(id.source_address, id.source_port),
                remote: SocketAddr::new(id.destination_address, id.destination_port),
            });
        }
        Ok(keys)
    }

    #[cfg(test)]
    mod tests {
        use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, TcpStream};
        use std::time::{Duration, Instant};

        use netlink_packet_core::{NetlinkHeader, NetlinkMessage};
        use netlink_packet_sock_diag::{
            constants::{AF_INET, AF_INET6},
            inet::{InetResponse, InetResponseHeader, SocketId},
            SockDiagMessage,
        };

        use super::*;

        fn encoded_response(source: IpAddr, destination: IpAddr) -> Vec<u8> {
            let family = if source.is_ipv4() { AF_INET } else { AF_INET6 };
            let response = InetResponse {
                header: InetResponseHeader {
                    family,
                    state: 7,
                    timer: None,
                    socket_id: SocketId {
                        source_port: 41_000,
                        destination_port: 443,
                        source_address: source,
                        destination_address: destination,
                        interface_id: 0,
                        cookie: [0; 8],
                    },
                    recv_queue: 0,
                    send_queue: 0,
                    uid: 1_000,
                    inode: 123,
                },
                nlas: Default::default(),
            };
            let mut message = NetlinkMessage::new(
                NetlinkHeader::default(),
                SockDiagMessage::InetResponse(Box::new(response)).into(),
            );
            message.finalize();
            let mut bytes = vec![0; message.buffer_len()];
            message.serialize(&mut bytes);
            bytes
        }

        #[test]
        fn parses_ipv4_and_ipv6_destroy_messages() {
            let v4 = encoded_response(
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5)),
            );
            let v6 = encoded_response(
                IpAddr::V6(Ipv6Addr::LOCALHOST),
                "2001:db8::5".parse().unwrap(),
            );
            let first = parse_destroy_messages(&v4).unwrap();
            let second = parse_destroy_messages(&v6).unwrap();
            assert_eq!(first[0].remote.to_string(), "203.0.113.5:443");
            assert_eq!(second[0].remote.to_string(), "[2001:db8::5]:443");
        }

        #[test]
        fn rejects_truncated_messages() {
            let bytes = encoded_response(
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5)),
            );
            assert!(parse_destroy_messages(&bytes[..bytes.len() - 1]).is_err());
        }

        #[test]
        #[ignore = "requires Linux socket-diagnostic multicast access"]
        fn receives_local_tcp_destroy_event() {
            let mut socket = Socket::new(NETLINK_SOCK_DIAG).unwrap();
            socket
                .bind(&NetlinkAddr::new(0, TCP_DESTROY_GROUPS))
                .unwrap();
            socket.set_non_blocking(true).unwrap();

            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let listener_addr = listener.local_addr().unwrap();
            let stream = TcpStream::connect(listener_addr).unwrap();
            let client_addr = stream.local_addr().unwrap();
            let accepted = listener.accept().unwrap().0;
            drop(stream);
            drop(accepted);
            drop(listener);

            let deadline = Instant::now() + Duration::from_secs(2);
            let mut buffer = vec![0u8; 64 * 1024];
            while Instant::now() < deadline {
                match socket.recv(&mut &mut buffer[..], 0) {
                    Ok(size) => {
                        let keys = parse_destroy_messages(&buffer[..size]).unwrap();
                        if keys.iter().any(|key| {
                            (key.local == client_addr && key.remote == listener_addr)
                                || (key.local == listener_addr && key.remote == client_addr)
                        }) {
                            return;
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("socket diagnostic receive failed: {error}"),
                }
            }
            panic!("did not receive a TCP destroy event");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(port: u16) -> SocketKey {
        SocketKey {
            protocol: crate::model::Protocol::Tcp,
            local: format!("127.0.0.1:{port}").parse().unwrap(),
            remote: "203.0.113.1:443".parse().unwrap(),
        }
    }

    #[test]
    fn bounded_inbox_drops_oldest_events() {
        let events = LifecycleEvents::default();
        for port in 0..=(EVENT_CAPACITY as u16) {
            push(&events.inner, key(port));
        }
        let drained = events.drain();
        assert_eq!(drained.len(), EVENT_CAPACITY);
        assert_eq!(events.status().dropped_events, 1);
        assert_eq!(drained.first().unwrap().local.port(), 1);
    }
}
