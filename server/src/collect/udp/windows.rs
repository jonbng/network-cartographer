use std::collections::BTreeSet;
use std::io;
use std::mem::{size_of, zeroed};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS, ERROR_INSUFFICIENT_BUFFER,
    ERROR_NO_MORE_ITEMS, HANDLE, NO_ERROR,
};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedUdpTable, MIB_UDP6ROW_OWNER_PID, MIB_UDPROW_OWNER_PID, UDP_TABLE_OWNER_PID,
};
use windows_sys::Win32::Networking::WinSock::{
    closesocket, getpeername, getsockname, getsockopt, WSACleanup, WSAStartup, AF_INET, AF_INET6,
    SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6, SOCKADDR_STORAGE, SOCK_DGRAM, SOL_SOCKET, SO_TYPE,
    WSADATA,
};
use windows_sys::Win32::System::Diagnostics::ProcessSnapshotting::{
    PssCaptureSnapshot, PssFreeSnapshot, PssWalkMarkerCreate, PssWalkMarkerFree, PssWalkSnapshot,
    HPSS, HPSSWALK, PSS_CAPTURE_HANDLES, PSS_HANDLE_ENTRY, PSS_WALK_HANDLES,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE, PROCESS_QUERY_INFORMATION,
};

use super::{UdpObservation, UdpSnapshot};

pub fn snapshot() -> io::Result<UdpSnapshot> {
    let worker = worker();
    let mut state = worker
        .lock()
        .map_err(|_| io::Error::other("Windows UDP worker lock poisoned"))?;
    state.last_requested = Instant::now();
    match state.latest.as_ref() {
        Some((at, Ok(snapshot))) if at.elapsed() <= Duration::from_secs(3) => Ok(snapshot.clone()),
        Some((at, Err(error))) if at.elapsed() <= Duration::from_secs(3) => {
            Err(io::Error::other(error.clone()))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "Windows connected UDP collector is starting",
        )),
    }
}

struct WorkerState {
    latest: Option<(Instant, Result<UdpSnapshot, String>)>,
    last_requested: Instant,
}

fn worker() -> &'static Arc<Mutex<WorkerState>> {
    static WORKER: OnceLock<Arc<Mutex<WorkerState>>> = OnceLock::new();
    WORKER.get_or_init(|| {
        let state = Arc::new(Mutex::new(WorkerState {
            latest: None,
            last_requested: Instant::now(),
        }));
        let background = Arc::clone(&state);
        std::thread::Builder::new()
            .name("windows-udp-peers".into())
            .spawn(move || loop {
                let requested = background
                    .lock()
                    .map(|state| state.last_requested.elapsed() <= Duration::from_secs(3))
                    .unwrap_or(false);
                if requested {
                    let result = scan().map_err(|error| error.to_string());
                    if let Ok(mut state) = background.lock() {
                        state.latest = Some((Instant::now(), result));
                    }
                }
                std::thread::sleep(Duration::from_secs(1));
            })
            .expect("spawn Windows UDP collector");
        state
    })
}

fn scan() -> io::Result<UdpSnapshot> {
    let pids = udp_owner_pids()?;

    let _winsock = Winsock::start()?;
    let mut observations = Vec::new();
    let mut skipped_processes = 0;
    for pid in pids {
        match inspect_process(pid) {
            Ok(mut found) => observations.append(&mut found),
            Err(_) => skipped_processes += 1,
        }
    }
    Ok(UdpSnapshot {
        observations,
        skipped_processes,
    })
}

fn udp_owner_pids() -> io::Result<BTreeSet<u32>> {
    let mut pids = BTreeSet::new();
    let v4 = udp_table(AF_INET as u32)?;
    let v4_count = v4.first().copied().unwrap_or_default() as usize;
    let v4_rows = unsafe {
        std::slice::from_raw_parts(v4.as_ptr().add(1).cast::<MIB_UDPROW_OWNER_PID>(), v4_count)
    };
    pids.extend(v4_rows.iter().map(|row| row.dwOwningPid));
    let v6 = udp_table(AF_INET6 as u32)?;
    let v6_count = v6.first().copied().unwrap_or_default() as usize;
    let v6_rows = unsafe {
        std::slice::from_raw_parts(v6.as_ptr().add(1).cast::<MIB_UDP6ROW_OWNER_PID>(), v6_count)
    };
    pids.extend(v6_rows.iter().map(|row| row.dwOwningPid));
    pids.remove(&0);
    Ok(pids)
}

fn udp_table(family: u32) -> io::Result<Vec<u32>> {
    let mut bytes = 0u32;
    let first = unsafe {
        GetExtendedUdpTable(
            std::ptr::null_mut(),
            &mut bytes,
            0,
            family,
            UDP_TABLE_OWNER_PID,
            0,
        )
    };
    if first != ERROR_INSUFFICIENT_BUFFER && first != NO_ERROR {
        return Err(io::Error::from_raw_os_error(first as i32));
    }
    let mut buffer = vec![0u32; (bytes as usize).div_ceil(size_of::<u32>())];
    let result = unsafe {
        GetExtendedUdpTable(
            buffer.as_mut_ptr().cast(),
            &mut bytes,
            0,
            family,
            UDP_TABLE_OWNER_PID,
            0,
        )
    };
    if result == NO_ERROR {
        Ok(buffer)
    } else {
        Err(io::Error::from_raw_os_error(result as i32))
    }
}

fn inspect_process(pid: u32) -> io::Result<Vec<UdpObservation>> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_DUP_HANDLE, 0, pid) };
    if process.is_null() {
        return Err(io::Error::last_os_error());
    }
    let process = OwnedHandle(process);

    let mut snapshot: HPSS = std::ptr::null_mut();
    let result = unsafe { PssCaptureSnapshot(process.0, PSS_CAPTURE_HANDLES, 0, &mut snapshot) };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let snapshot = OwnedSnapshot {
        process: process.0,
        snapshot,
    };

    let mut marker: HPSSWALK = std::ptr::null_mut();
    let result = unsafe { PssWalkMarkerCreate(std::ptr::null(), &mut marker) };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let marker = OwnedMarker(marker);
    let mut out = Vec::new();
    loop {
        let mut entry: PSS_HANDLE_ENTRY = unsafe { zeroed() };
        let result = unsafe {
            PssWalkSnapshot(
                snapshot.snapshot,
                PSS_WALK_HANDLES,
                marker.0,
                &mut entry as *mut _ as *mut _,
                size_of::<PSS_HANDLE_ENTRY>() as u32,
            )
        };
        if result == ERROR_NO_MORE_ITEMS {
            break;
        }
        if result != 0 {
            return Err(io::Error::from_raw_os_error(result as i32));
        }
        if let Some((local, remote)) = inspect_handle(process.0, entry.Handle) {
            out.push(UdpObservation {
                local,
                remote,
                pids: vec![pid],
            });
        }
    }
    Ok(out)
}

fn inspect_handle(process: HANDLE, source: HANDLE) -> Option<(SocketAddr, SocketAddr)> {
    let mut duplicated: HANDLE = std::ptr::null_mut();
    let duplicated_ok = unsafe {
        DuplicateHandle(
            process,
            source,
            GetCurrentProcess(),
            &mut duplicated,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if duplicated_ok == 0 || duplicated.is_null() {
        return None;
    }

    let socket = duplicated as usize;
    let mut socket_type = 0i32;
    let mut socket_type_len = size_of::<i32>() as i32;
    let is_socket = unsafe {
        getsockopt(
            socket,
            SOL_SOCKET,
            SO_TYPE,
            &mut socket_type as *mut _ as *mut u8,
            &mut socket_type_len,
        ) == 0
    };
    if !is_socket {
        unsafe { CloseHandle(duplicated) };
        return None;
    }
    let socket = OwnedSocket(socket);
    if socket_type != SOCK_DGRAM {
        return None;
    }

    let local = socket_address(socket.0, false)?;
    let remote = socket_address(socket.0, true)?;
    if remote.ip().is_unspecified() || remote.port() == 0 {
        return None;
    }
    Some((local, remote))
}

fn socket_address(socket: usize, peer: bool) -> Option<SocketAddr> {
    let mut storage: SOCKADDR_STORAGE = unsafe { zeroed() };
    let mut length = size_of::<SOCKADDR_STORAGE>() as i32;
    let result = unsafe {
        if peer {
            getpeername(socket, &mut storage as *mut _ as *mut SOCKADDR, &mut length)
        } else {
            getsockname(socket, &mut storage as *mut _ as *mut SOCKADDR, &mut length)
        }
    };
    if result != 0 {
        return None;
    }
    unsafe {
        match storage.ss_family {
            AF_INET => {
                let address = &*(&storage as *const _ as *const SOCKADDR_IN);
                let bytes = address.sin_addr.S_un.S_un_b;
                Some(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(
                        bytes.s_b1, bytes.s_b2, bytes.s_b3, bytes.s_b4,
                    )),
                    u16::from_be(address.sin_port),
                ))
            }
            AF_INET6 => {
                let address = &*(&storage as *const _ as *const SOCKADDR_IN6);
                Some(SocketAddr::new(
                    IpAddr::V6(Ipv6Addr::from(address.sin6_addr.u.Byte)),
                    u16::from_be(address.sin6_port),
                ))
            }
            _ => None,
        }
    }
}

struct Winsock;

impl Winsock {
    fn start() -> io::Result<Self> {
        let mut data: WSADATA = unsafe { zeroed() };
        let result = unsafe { WSAStartup(0x0202, &mut data) };
        if result == 0 {
            Ok(Self)
        } else {
            Err(io::Error::from_raw_os_error(result))
        }
    }
}

impl Drop for Winsock {
    fn drop(&mut self) {
        unsafe { WSACleanup() };
    }
}

struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

struct OwnedSocket(usize);
impl Drop for OwnedSocket {
    fn drop(&mut self) {
        unsafe { closesocket(self.0) };
    }
}

struct OwnedMarker(HPSSWALK);
impl Drop for OwnedMarker {
    fn drop(&mut self) {
        unsafe { PssWalkMarkerFree(self.0) };
    }
}

struct OwnedSnapshot {
    process: HANDLE,
    snapshot: HPSS,
}
impl Drop for OwnedSnapshot {
    fn drop(&mut self) {
        unsafe { PssFreeSnapshot(self.process, self.snapshot) };
    }
}
