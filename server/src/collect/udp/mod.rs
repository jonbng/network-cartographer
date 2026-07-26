#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::net::SocketAddr;

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[derive(Debug, Clone)]
pub struct UdpObservation {
    pub local: SocketAddr,
    pub remote: SocketAddr,
    pub pids: Vec<u32>,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[derive(Debug, Clone, Default)]
pub struct UdpSnapshot {
    pub observations: Vec<UdpObservation>,
    pub skipped_processes: usize,
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
pub use macos::snapshot;
#[cfg(target_os = "macos")]
pub use macos::tcp_snapshot;
#[cfg(target_os = "windows")]
pub use windows::snapshot;
