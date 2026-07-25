//! Process elevation detection (best-effort, cross-platform).

#[cfg(unix)]
pub fn is_elevated() -> bool {
    // SAFETY: geteuid is always safe to call
    unsafe { libc::geteuid() == 0 }
}

#[cfg(windows)]
pub fn is_elevated() -> bool {
    false // banner still useful; full admin check needs extra windows APIs
}

#[cfg(not(any(unix, windows)))]
pub fn is_elevated() -> bool {
    false
}

pub fn elevation_hint() -> Option<&'static str> {
    if is_elevated() {
        None
    } else {
        Some("Not elevated: process names and TCP/ICMP traceroute may be incomplete. Re-run with admin/sudo for better results.")
    }
}
