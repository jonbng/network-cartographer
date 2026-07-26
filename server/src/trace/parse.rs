use std::net::IpAddr;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct Hop {
    pub ttl: u8,
    pub addr: Option<IpAddr>,
    pub rtt_ms: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct TraceResult {
    #[allow(dead_code)]
    pub target: IpAddr,
    pub hops: Vec<Hop>,
    pub finished_at: Instant,
    pub error: Option<String>,
}

impl TraceResult {
    pub fn hop_count(&self) -> usize {
        self.hops.len()
    }

    /// Best RTT on the last hop that answered (approx end-to-end).
    pub fn final_rtt_ms(&self) -> Option<f64> {
        self.hops.iter().rev().find_map(|h| h.rtt_ms)
    }

    /// Whether traceroute received a reply from the exact requested target.
    pub fn reached_target(&self) -> bool {
        self.hops.iter().any(|hop| hop.addr == Some(self.target))
    }

    /// RTT reported by the target itself. Unlike `final_rtt_ms`, this is safe
    /// to describe as end-to-end latency.
    pub fn target_rtt_ms(&self) -> Option<f64> {
        self.hops
            .iter()
            .find(|hop| hop.addr == Some(self.target))
            .and_then(|hop| hop.rtt_ms)
    }
}

/// Parse output from traceroute / tracert / tracepath (`-n` numeric form preferred).
pub fn parse_traceroute_output(target: IpAddr, stdout: &str) -> TraceResult {
    let mut hops = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Skip headers
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("traceroute")
            || lower.starts_with("tracing route")
            || lower.starts_with("over a maximum")
            || lower.starts_with("tracepath")
        {
            continue;
        }

        if let Some(hop) = parse_hop_line(line) {
            hops.push(hop);
        }
    }

    TraceResult {
        target,
        hops,
        finished_at: Instant::now(),
        error: None,
    }
}

pub(super) fn parse_hop_line(line: &str) -> Option<Hop> {
    // Common patterns:
    // " 1  10.16.96.2  2.025 ms"
    // " 3  * * *"
    // " 1    <1 ms    <1 ms    <1 ms  192.168.1.1"  (tracert)
    // " 1:  192.168.1.1  1.234ms" (tracepath-ish)
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    // Hop number: first token, may end with ':'
    let ttl_str = tokens[0].trim_end_matches(':');
    let ttl: u8 = ttl_str.parse().ok()?;
    if ttl == 0 {
        return None;
    }

    // Timeout-only line
    if tokens.len() >= 2 && tokens[1..].iter().all(|t| *t == "*" || t.ends_with('*')) {
        return Some(Hop {
            ttl,
            addr: None,
            rtt_ms: None,
        });
    }

    let mut addr: Option<IpAddr> = None;
    let mut rtts: Vec<f64> = Vec::new();

    let mut i = 1;
    while i < tokens.len() {
        let t = tokens[i];

        if t == "*" {
            i += 1;
            continue;
        }

        // "<1" ms style from tracert
        if t.starts_with('<') {
            if let Ok(v) = t.trim_start_matches('<').parse::<f64>() {
                rtts.push(v.max(0.1));
            }
            // optional following "ms"
            if i + 1 < tokens.len() && tokens[i + 1].eq_ignore_ascii_case("ms") {
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        // IP address. BSD traceroute6 can append an interface scope to
        // link-local hops (for example, fe80::1%en0).
        if let Some(ip) = parse_ip_token(t) {
            addr = Some(ip);
            i += 1;
            continue;
        }

        // RTT like "2.025" followed by "ms", or "2.025ms"
        if i + 1 < tokens.len() && tokens[i + 1].eq_ignore_ascii_case("ms") {
            if let Ok(ms_val) = t.parse::<f64>() {
                rtts.push(ms_val);
                i += 2;
                continue;
            }
        }
        if let Some(ms_val) = parse_rtt_token(t) {
            rtts.push(ms_val);
            if i + 1 < tokens.len() && tokens[i + 1].eq_ignore_ascii_case("ms") {
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        if t.eq_ignore_ascii_case("ms") {
            i += 1;
            continue;
        }

        // Hostname (when -n not used); skip unless we already have IP
        i += 1;
    }

    // If only timeouts after hop number
    if addr.is_none() && rtts.is_empty() {
        // Might be "* * *" already handled; otherwise treat as timeout hop
        if tokens.contains(&"*") {
            return Some(Hop {
                ttl,
                addr: None,
                rtt_ms: None,
            });
        }
        return None;
    }

    let rtt_ms = if rtts.is_empty() {
        None
    } else {
        Some(rtts.iter().sum::<f64>() / rtts.len() as f64)
    };

    Some(Hop { ttl, addr, rtt_ms })
}

fn parse_ip_token(token: &str) -> Option<IpAddr> {
    let token = token.trim_matches(|c| matches!(c, '(' | ')' | '[' | ']' | ','));
    token.parse().ok().or_else(|| {
        let (addr, _scope) = token.split_once('%')?;
        addr.parse().ok()
    })
}

fn parse_rtt_token(t: &str) -> Option<f64> {
    let t = t.trim();
    if let Some(stripped) = t.strip_suffix("ms").or_else(|| t.strip_suffix("MS")) {
        return stripped.parse().ok();
    }
    // bare number that looks like rtt (has decimal or small int)
    if t.contains('.') {
        return t.parse().ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn parse_linux_traceroute_n() {
        let out = "\
traceroute to 1.1.1.1 (1.1.1.1), 5 hops max, 60 byte packets
 1  10.16.96.2  2.025 ms
 2  10.64.1.113  3.819 ms
 3  * * *
 4  1.1.1.1  11.937 ms
";
        let target = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));
        let r = parse_traceroute_output(target, out);
        assert_eq!(r.hops.len(), 4);
        assert_eq!(
            r.hops[0].addr,
            Some(IpAddr::V4(Ipv4Addr::new(10, 16, 96, 2)))
        );
        assert!(r.hops[0].rtt_ms.unwrap() > 2.0);
        assert!(r.hops[2].addr.is_none());
        assert_eq!(r.hops[3].addr, Some(target));
    }

    #[test]
    fn parse_windows_tracert() {
        let out = "\
Tracing route to 1.1.1.1 over a maximum of 30 hops

  1    <1 ms    <1 ms    <1 ms  192.168.1.1
  2     4 ms     3 ms     4 ms  10.0.0.1
  3     *        *        *     Request timed out.
  4    12 ms    11 ms    12 ms  1.1.1.1
";
        let target = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));
        let r = parse_traceroute_output(target, out);
        assert!(r.hops.len() >= 3);
        assert_eq!(
            r.hops[0].addr,
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)))
        );
    }

    #[test]
    fn parse_single_probe_line() {
        let hop = parse_hop_line(" 5  137.164.23.144  7.781 ms").unwrap();
        assert_eq!(hop.ttl, 5);
        assert!(hop.rtt_ms.unwrap() > 7.0);
    }

    #[test]
    fn parse_macos_traceroute_n() {
        // BSD traceroute -n / -I numeric form (macOS)
        let out = "\
traceroute to 1.1.1.1 (1.1.1.1), 20 hops max, 40 byte packets
 1  192.168.1.1  1.234 ms
 2  10.0.0.1  4.560 ms
 3  * * *
 4  1.1.1.1  12.100 ms
";
        let target = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));
        let r = parse_traceroute_output(target, out);
        assert_eq!(r.hops.len(), 4);
        assert_eq!(
            r.hops[0].addr,
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)))
        );
        assert!(r.hops[0].rtt_ms.unwrap() > 1.0);
        assert!(r.hops[2].addr.is_none());
        assert_eq!(r.hops[3].addr, Some(target));
    }

    #[test]
    fn parse_macos_integer_rtt_and_scoped_ipv6() {
        let hop = parse_hop_line(" 1  fe80::1%en0  1 ms").unwrap();
        assert_eq!(hop.addr, Some("fe80::1".parse().unwrap()));
        assert_eq!(hop.rtt_ms, Some(1.0));
    }

    #[test]
    fn parse_windows_request_timed_out_line() {
        let hop = parse_hop_line("  3     *        *        *     Request timed out.").unwrap();
        assert_eq!(hop.ttl, 3);
        assert!(hop.addr.is_none());
        assert!(hop.rtt_ms.is_none());
    }

    #[test]
    fn distinguishes_target_rtt_from_last_reply() {
        let target = "1.1.1.1".parse().unwrap();
        let partial = parse_traceroute_output(
            target,
            "1  192.168.1.1  1.0 ms\n2  203.0.113.9  8.0 ms\n3  * * *\n",
        );
        assert!(!partial.reached_target());
        assert_eq!(partial.target_rtt_ms(), None);
        assert_eq!(partial.final_rtt_ms(), Some(8.0));

        let complete =
            parse_traceroute_output(target, "1  192.168.1.1  1.0 ms\n2  1.1.1.1  12.0 ms\n");
        assert!(complete.reached_target());
        assert_eq!(complete.target_rtt_ms(), Some(12.0));
    }

    #[test]
    fn recognizes_ipv6_target_reply() {
        let target = "2606:4700:4700::1111".parse().unwrap();
        let trace = parse_traceroute_output(
            target,
            "1  fe80::1%en0  1 ms\n2  2606:4700:4700::1111  18 ms\n",
        );
        assert!(trace.reached_target());
        assert_eq!(trace.target_rtt_ms(), Some(18.0));
    }
}
