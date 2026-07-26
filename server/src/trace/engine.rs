use std::collections::{HashMap, HashSet, VecDeque};
use std::net::IpAddr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::command::run_command_timeout;
use super::parse::{parse_hop_line, parse_traceroute_output, Hop, TraceResult};
use crate::model::is_local_or_private;

#[derive(Debug, Clone)]
struct ProbeConfig {
    max_hops: u8,
    timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct TraceConfig {
    pub enabled: bool,
    pub max_concurrent: usize,
    pub cache_ttl: Duration,
    pub max_hops: u8,
    pub process_timeout: Duration,
    /// Never traceroute private/local IPs (default true).
    pub skip_private: bool,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            // Traceroutes spend most of their lifetime waiting on network
            // replies, so a moderately larger pool drains bursts much faster.
            max_concurrent: 20,
            cache_ttl: Duration::from_secs(900),
            // Most paths settle well under 20 hops; saves probe time
            max_hops: 20,
            // Kill hung traces sooner
            process_timeout: Duration::from_secs(28),
            skip_private: true,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TraceStatus {
    Idle,
    Queued {
        previous: Option<TraceResult>,
    },
    Running {
        previous: Option<TraceResult>,
        progress: TraceProgress,
    },
    Done(TraceResult),
    Stale {
        result: TraceResult,
        message: String,
        at: Instant,
    },
    Failed {
        message: String,
        at: Instant,
    },
}

#[derive(Debug, Clone)]
pub struct TraceProgress {
    pub target: IpAddr,
    pub hops: Vec<Hop>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TraceStats {
    pub queued: usize,
    pub running: usize,
    pub done: usize,
    pub failed: usize,
}

enum JobEvent {
    Progress {
        ip: IpAddr,
        generation: u64,
        progress: TraceProgress,
    },
    Finished {
        ip: IpAddr,
        generation: u64,
        result: TraceResult,
    },
}

#[derive(Debug, Clone, Copy)]
struct TraceJob {
    ip: IpAddr,
    generation: u64,
}

/// Background traceroute queue with cache, shared by IP across apps.
pub struct TraceEngine {
    cfg: TraceConfig,
    cache: HashMap<IpAddr, TraceStatus>,
    queue: VecDeque<IpAddr>,
    pending: HashSet<IpAddr>,
    running: HashSet<IpAddr>,
    generation: u64,
    job_tx: Option<Sender<TraceJob>>,
    result_rx: Receiver<JobEvent>,
}

impl TraceEngine {
    pub fn new(cfg: TraceConfig) -> Self {
        let (result_tx, result_rx) = mpsc::channel::<JobEvent>();
        let (job_tx, job_rx) = mpsc::channel::<TraceJob>();
        let job_rx = Arc::new(Mutex::new(job_rx));
        if cfg.enabled {
            let workers = cfg.max_concurrent.max(1);
            let probe = ProbeConfig {
                max_hops: cfg.max_hops,
                timeout: cfg.process_timeout,
            };
            for i in 0..workers {
                let job_rx = Arc::clone(&job_rx);
                let result_tx = result_tx.clone();
                let probe = probe.clone();
                thread::Builder::new()
                    .name(format!("trace-worker-{i}"))
                    .spawn(move || worker_loop(job_rx, result_tx, probe))
                    .expect("spawn trace worker");
            }
        }

        Self {
            cfg,
            cache: HashMap::new(),
            queue: VecDeque::new(),
            pending: HashSet::new(),
            running: HashSet::new(),
            generation: 0,
            job_tx: Some(job_tx),
            result_rx,
        }
    }

    pub fn enabled(&self) -> bool {
        self.cfg.enabled
    }

    pub fn request(&mut self, ip: IpAddr) {
        if !self.cfg.enabled {
            return;
        }
        if self.cfg.skip_private && is_local_or_private(ip) {
            return;
        }
        if !is_valid_target(ip) {
            return;
        }

        self.poll();

        if let Some(status) = self.cache.get(&ip) {
            match status {
                TraceStatus::Done(r) if r.finished_at.elapsed() < self.cfg.cache_ttl => return,
                TraceStatus::Failed { at, .. } if at.elapsed() < Duration::from_secs(300) => return,
                TraceStatus::Stale { at, .. } if at.elapsed() < Duration::from_secs(300) => return,
                TraceStatus::Queued { .. } | TraceStatus::Running { .. } => return,
                _ => {}
            }
        }

        if self.pending.contains(&ip) || self.running.contains(&ip) {
            return;
        }

        self.pending.insert(ip);
        self.queue.push_back(ip);
        self.cache
            .insert(ip, TraceStatus::Queued { previous: None });
        self.dispatch();
    }

    pub fn force(&mut self, ip: IpAddr) {
        if !self.cfg.enabled
            || (self.cfg.skip_private && is_local_or_private(ip))
            || !is_valid_target(ip)
        {
            return;
        }
        self.poll();
        // An in-flight or already queued probe is already the freshest result
        // available. Repeated UI clicks must not build a duplicate queue.
        if self.pending.contains(&ip) || self.running.contains(&ip) {
            return;
        }
        let previous = previous_result(self.cache.remove(&ip));
        self.pending.insert(ip);
        self.queue.push_back(ip);
        self.cache.insert(ip, TraceStatus::Queued { previous });
        self.dispatch();
    }

    pub fn force_many(&mut self, ips: impl IntoIterator<Item = IpAddr>) {
        for ip in ips {
            self.force(ip);
        }
    }

    pub fn get(&self, ip: IpAddr) -> TraceStatus {
        self.cache.get(&ip).cloned().unwrap_or(TraceStatus::Idle)
    }

    pub fn poll(&mut self) {
        while let Ok(event) = self.result_rx.try_recv() {
            match event {
                JobEvent::Progress {
                    ip,
                    generation,
                    progress,
                } => {
                    if generation != self.generation || !self.running.contains(&ip) {
                        continue;
                    }
                    let previous = previous_result(self.cache.remove(&ip));
                    self.cache
                        .insert(ip, TraceStatus::Running { previous, progress });
                }
                JobEvent::Finished {
                    ip,
                    generation,
                    result,
                } => {
                    if generation != self.generation {
                        continue;
                    }
                    self.running.remove(&ip);
                    self.pending.remove(&ip);
                    let status = if result.error.is_some() && result.hops.is_empty() {
                        let message = result.error.clone().unwrap_or_else(|| "failed".into());
                        match previous_result(self.cache.remove(&ip)) {
                            Some(result) => TraceStatus::Stale {
                                result,
                                message,
                                at: Instant::now(),
                            },
                            None => TraceStatus::Failed {
                                message,
                                at: Instant::now(),
                            },
                        }
                    } else {
                        TraceStatus::Done(result)
                    };
                    self.cache.insert(ip, status);
                }
            }
        }
        self.dispatch();
    }

    pub fn stats(&self) -> TraceStats {
        let mut s = TraceStats::default();
        for st in self.cache.values() {
            match st {
                TraceStatus::Queued { .. } => s.queued += 1,
                TraceStatus::Running { .. } => s.running += 1,
                TraceStatus::Done(_) => s.done += 1,
                TraceStatus::Stale { .. } => {
                    s.done += 1;
                    s.failed += 1;
                }
                TraceStatus::Failed { .. } => s.failed += 1,
                TraceStatus::Idle => {}
            }
        }
        // queue may have pending not yet in cache as running
        s.queued = s.queued.max(self.queue.len());
        s.running = s.running.max(self.running.len());
        s
    }

    pub fn clear_cache(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.cache.clear();
        self.queue.clear();
        self.pending.clear();
        // leave in-flight workers; results will repopulate cache
    }

    /// Re-probe active destinations after a network transition without making
    /// the useful, previously completed globe disappear.
    pub fn refresh_many(&mut self, ips: impl IntoIterator<Item = IpAddr>) {
        if !self.cfg.enabled {
            return;
        }
        self.poll();
        self.generation = self.generation.wrapping_add(1);
        self.queue.clear();
        self.pending.clear();
        self.running.clear();

        let mut targets = HashSet::new();
        for ip in ips {
            if (self.cfg.skip_private && is_local_or_private(ip)) || !is_valid_target(ip) {
                continue;
            }
            if !targets.insert(ip) {
                continue;
            }
            let previous = previous_result(self.cache.remove(&ip));
            self.pending.insert(ip);
            self.queue.push_back(ip);
            self.cache.insert(ip, TraceStatus::Queued { previous });
        }
        self.cache.retain(|ip, _| targets.contains(ip));
        self.dispatch();
    }

    fn dispatch(&mut self) {
        let Some(tx) = &self.job_tx else {
            return;
        };
        let max = self.cfg.max_concurrent.max(1);
        while self.running.len() < max {
            let Some(ip) = self.queue.pop_front() else {
                break;
            };
            self.running.insert(ip);
            let previous = previous_result(self.cache.remove(&ip));
            self.cache.insert(
                ip,
                TraceStatus::Running {
                    previous,
                    progress: TraceProgress {
                        target: ip,
                        hops: Vec::new(),
                    },
                },
            );
            if tx
                .send(TraceJob {
                    ip,
                    generation: self.generation,
                })
                .is_err()
            {
                self.running.remove(&ip);
                let status = match previous_result(self.cache.remove(&ip)) {
                    Some(result) => TraceStatus::Stale {
                        result,
                        message: "trace workers stopped".into(),
                        at: Instant::now(),
                    },
                    None => TraceStatus::Failed {
                        message: "trace workers stopped".into(),
                        at: Instant::now(),
                    },
                };
                self.cache.insert(ip, status);
                break;
            }
        }
    }
}

fn worker_loop(
    job_rx: Arc<Mutex<Receiver<TraceJob>>>,
    result_tx: Sender<JobEvent>,
    probe: ProbeConfig,
) {
    loop {
        let job = {
            let guard = match job_rx.lock() {
                Ok(g) => g,
                Err(_) => break,
            };
            match guard.recv() {
                Ok(job) => job,
                Err(_) => break,
            }
        };

        let result = execute_trace(job.ip, &probe, |progress| {
            let _ = result_tx.send(JobEvent::Progress {
                ip: job.ip,
                generation: job.generation,
                progress,
            });
        });

        if result_tx
            .send(JobEvent::Finished {
                ip: job.ip,
                generation: job.generation,
                result,
            })
            .is_err()
        {
            break;
        }
    }
}

fn previous_result(status: Option<TraceStatus>) -> Option<TraceResult> {
    match status? {
        TraceStatus::Queued { previous } | TraceStatus::Running { previous, .. } => previous,
        TraceStatus::Done(result) => Some(result),
        TraceStatus::Stale { result, .. } => Some(result),
        _ => None,
    }
}

fn execute_trace<F>(target: IpAddr, probe: &ProbeConfig, mut on_progress: F) -> TraceResult
where
    F: FnMut(TraceProgress),
{
    let attempts = commands_for(target, probe);
    let mut last_err = String::from("traceroute failed");
    let mut best: Option<TraceResult> = None;
    let mut best_live: Option<TraceProgress> = None;

    for (program, args) in attempts {
        let mut attempt_hops = Vec::new();
        match run_command_timeout(&program, &args, probe.timeout, |line| {
            let Some(hop) = parse_hop_line(line) else {
                return;
            };
            upsert_hop(&mut attempt_hops, hop);
            let candidate = TraceProgress {
                target,
                hops: attempt_hops.clone(),
            };
            if progress_rank(&candidate)
                >= best_live
                    .as_ref()
                    .map(progress_rank)
                    .unwrap_or((0, 0, 0, 0))
            {
                best_live = Some(candidate.clone());
                on_progress(candidate);
            }
        }) {
            Ok(output) => {
                let mut result = parse_traceroute_output(target, &output.output);
                let mut normalized = Vec::with_capacity(result.hops.len());
                for hop in result.hops.drain(..) {
                    upsert_hop(&mut normalized, hop);
                }
                result.hops = normalized;
                result.error = output.warning;
                if result.hops.is_empty() {
                    result.error = Some("no hops parsed".into());
                    last_err = format!("{program}: no hops");
                    continue;
                }
                // Prefer the run that actually reached the target (or most answered hops)
                let replace = match &best {
                    None => true,
                    Some(b) => trace_rank(&result) > trace_rank(b),
                };
                if replace {
                    best = Some(result);
                }
                // Good enough: reached destination with several hops
                if let Some(ref b) = best {
                    if path_reached_target(b, target) && b.hops.len() >= 3 {
                        return best.unwrap();
                    }
                }
            }
            Err(e) => last_err = format!("{program}: {e}"),
        }
    }

    best.unwrap_or(TraceResult {
        target,
        hops: vec![],
        finished_at: Instant::now(),
        error: Some(last_err),
    })
}

fn path_reached_target(r: &TraceResult, target: IpAddr) -> bool {
    r.target == target && r.reached_target()
}

fn trace_rank(result: &TraceResult) -> (u8, usize, u8, usize) {
    let reached = u8::from(result.reached_target());
    let answered = result.hops.iter().filter(|hop| hop.addr.is_some()).count();
    let max_ttl = result.hops.iter().map(|hop| hop.ttl).max().unwrap_or(0);
    (reached, answered, max_ttl, result.hops.len())
}

fn progress_rank(progress: &TraceProgress) -> (u8, usize, u8, usize) {
    let reached = u8::from(
        progress
            .hops
            .iter()
            .any(|hop| hop.addr == Some(progress.target)),
    );
    let answered = progress
        .hops
        .iter()
        .filter(|hop| hop.addr.is_some())
        .count();
    let max_ttl = progress.hops.iter().map(|hop| hop.ttl).max().unwrap_or(0);
    (reached, answered, max_ttl, progress.hops.len())
}

fn upsert_hop(hops: &mut Vec<Hop>, hop: Hop) {
    match hops.binary_search_by_key(&hop.ttl, |existing| existing.ttl) {
        Ok(index) => hops[index] = hop,
        Err(index) => hops.insert(index, hop),
    }
}

/// Build OS-appropriate traceroute/tracert command attempts for `target`.
///
/// Linux: unprivileged UDP traceroute → tracepath fallback.
/// macOS / other BSD-like Unix: unprivileged UDP traceroute.
/// Windows: `tracert`.
#[allow(clippy::needless_return)] // cfg branches are clearer as explicit returns.
fn commands_for(target: IpAddr, probe: &ProbeConfig) -> Vec<(String, Vec<String>)> {
    let ip = target.to_string();
    let max = probe.max_hops.to_string();

    #[cfg(target_os = "windows")]
    {
        let mut args = Vec::new();
        // Prefer address family flag when probing IPv6 (also fine with literal v6).
        if target.is_ipv6() {
            args.push("-6".into());
        }
        args.extend([
            "-d".into(),
            "-w".into(),
            "1000".into(),
            "-h".into(),
            max,
            ip,
        ]);
        return vec![("tracert".into(), args)];
    }

    #[cfg(target_os = "linux")]
    {
        // Both attempts work as a normal user. Do not add TCP (`-T`) or ICMP
        // (`-I`) probes here: those modes are not consistently available to
        // a normal user.
        return vec![
            (
                "traceroute".into(),
                vec![
                    "-n".into(),
                    "-w".into(),
                    "1".into(),
                    "-q".into(),
                    "1".into(),
                    "-N".into(),
                    "32".into(),
                    "-m".into(),
                    max.clone(),
                    ip.clone(),
                ],
            ),
            ("tracepath".into(), vec!["-n".into(), "-m".into(), max, ip]),
        ];
    }

    // macOS ships separate IPv4/IPv6 binaries. Both use BSD flags.
    #[cfg(target_os = "macos")]
    {
        return macos_commands(target, max, ip);
    }

    // Other non-Linux Unix targets use BSD traceroute flags.
    #[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
    {
        return vec![(
            "traceroute".into(),
            vec![
                "-n".into(),
                "-w".into(),
                "1".into(),
                "-q".into(),
                "1".into(),
                "-m".into(),
                max,
                ip,
            ],
        )];
    }

    // Unreachable on supported targets; keeps the type checker happy if cfgs change.
    #[cfg(not(any(target_os = "windows", target_os = "linux", unix)))]
    {
        let _ = (ip, max);
        vec![]
    }
}

#[cfg(any(target_os = "macos", test))]
fn macos_commands(target: IpAddr, max: String, ip: String) -> Vec<(String, Vec<String>)> {
    let program = if target.is_ipv6() {
        "traceroute6"
    } else {
        "traceroute"
    };
    vec![(
        program.into(),
        vec![
            "-n".into(),
            "-w".into(),
            "1".into(),
            "-q".into(),
            "1".into(),
            "-m".into(),
            max,
            ip,
        ],
    )]
}

fn is_valid_target(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_unspecified()
                || v4.is_loopback()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_link_local())
        }
        IpAddr::V6(v6) => {
            !(v6.is_unspecified()
                || v6.is_loopback()
                || v6.is_multicast()
                || v6.is_unicast_link_local())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn probe() -> ProbeConfig {
        ProbeConfig {
            max_hops: 20,
            timeout: Duration::from_secs(28),
        }
    }

    fn target_v4() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_commands_are_unprivileged() {
        let cmds = commands_for(target_v4(), &probe());
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].0, "traceroute");
        assert!(cmds[0].1.iter().any(|a| a == "-N"));
        assert!(!cmds[0].1.iter().any(|a| a == "-T" || a == "-I"));
        assert!(cmds.iter().any(|(p, _)| p == "tracepath"));
    }

    #[test]
    fn macos_commands_have_no_linux_only_flags() {
        let target = target_v4();
        let cmds = macos_commands(target, "20".into(), target.to_string());
        assert!(!cmds.is_empty());
        for (program, args) in &cmds {
            assert_eq!(program, "traceroute");
            assert!(
                !args.iter().any(|a| a == "-T" || a == "-N"),
                "macOS must not use Linux-only flags: {args:?}"
            );
            assert_ne!(program.as_str(), "tracepath");
        }
        assert_eq!(cmds.len(), 1);
        assert!(!cmds[0].1.iter().any(|a| a == "-I"));
    }

    #[test]
    fn macos_ipv6_uses_traceroute6() {
        let target: IpAddr = "2606:4700:4700::1111".parse().unwrap();
        let cmds = macos_commands(target, "20".into(), target.to_string());
        assert!(!cmds.is_empty());
        assert!(cmds.iter().all(|(program, _)| program == "traceroute6"));
    }

    fn completed(target: IpAddr) -> TraceResult {
        TraceResult {
            target,
            hops: vec![crate::trace::Hop {
                ttl: 1,
                addr: Some(target),
                rtt_ms: Some(10.0),
            }],
            finished_at: Instant::now(),
            error: None,
        }
    }

    #[test]
    fn network_refresh_preserves_completed_route() {
        let mut engine = TraceEngine::new(TraceConfig {
            enabled: false,
            ..TraceConfig::default()
        });
        engine.cfg.enabled = true;
        engine.job_tx = None;
        let target = target_v4();
        engine
            .cache
            .insert(target, TraceStatus::Done(completed(target)));

        engine.refresh_many([target]);

        assert!(matches!(
            engine.get(target),
            TraceStatus::Queued { previous: Some(_) }
        ));
        assert_eq!(engine.generation, 1);
    }

    #[test]
    fn result_from_previous_network_generation_is_ignored() {
        let mut engine = TraceEngine::new(TraceConfig {
            enabled: false,
            ..TraceConfig::default()
        });
        engine.cfg.enabled = true;
        engine.job_tx = None;
        let target = target_v4();
        engine
            .cache
            .insert(target, TraceStatus::Done(completed(target)));
        engine.refresh_many([target]);

        let (sender, receiver) = mpsc::channel();
        engine.result_rx = receiver;
        sender
            .send(JobEvent::Finished {
                ip: target,
                generation: 0,
                result: completed(target),
            })
            .unwrap();
        engine.poll();

        assert!(matches!(
            engine.get(target),
            TraceStatus::Queued { previous: Some(_) }
        ));
    }

    #[test]
    fn fresh_progress_replaces_visible_hops_but_retains_refresh_fallback() {
        let mut engine = TraceEngine::new(TraceConfig {
            enabled: false,
            ..TraceConfig::default()
        });
        engine.cfg.enabled = true;
        engine.job_tx = None;
        let target = target_v4();
        let previous = completed(target);
        engine.running.insert(target);
        engine.cache.insert(
            target,
            TraceStatus::Running {
                previous: Some(previous),
                progress: TraceProgress {
                    target,
                    hops: Vec::new(),
                },
            },
        );
        let (sender, receiver) = mpsc::channel();
        engine.result_rx = receiver;
        sender
            .send(JobEvent::Progress {
                ip: target,
                generation: engine.generation,
                progress: TraceProgress {
                    target,
                    hops: vec![Hop {
                        ttl: 1,
                        addr: Some("192.0.2.1".parse().unwrap()),
                        rtt_ms: Some(1.0),
                    }],
                },
            })
            .unwrap();

        engine.poll();

        match engine.get(target) {
            TraceStatus::Running { previous, progress } => {
                assert!(previous.is_some());
                assert_eq!(progress.hops.len(), 1);
                assert_eq!(progress.hops[0].ttl, 1);
            }
            status => panic!("unexpected trace status: {status:?}"),
        }
    }

    #[test]
    fn failed_refresh_without_fresh_hops_restores_previous_route_as_stale() {
        let mut engine = TraceEngine::new(TraceConfig {
            enabled: false,
            ..TraceConfig::default()
        });
        engine.cfg.enabled = true;
        engine.job_tx = None;
        let target = target_v4();
        engine.running.insert(target);
        engine.cache.insert(
            target,
            TraceStatus::Running {
                previous: Some(completed(target)),
                progress: TraceProgress {
                    target,
                    hops: Vec::new(),
                },
            },
        );
        let (sender, receiver) = mpsc::channel();
        engine.result_rx = receiver;
        sender
            .send(JobEvent::Finished {
                ip: target,
                generation: engine.generation,
                result: TraceResult {
                    target,
                    hops: Vec::new(),
                    finished_at: Instant::now(),
                    error: Some("timed out".into()),
                },
            })
            .unwrap();

        engine.poll();

        assert!(matches!(
            engine.get(target),
            TraceStatus::Stale { result, message, .. }
                if result.reached_target() && message == "timed out"
        ));
    }

    #[test]
    fn partial_result_with_error_remains_a_completed_partial_route() {
        let mut engine = TraceEngine::new(TraceConfig {
            enabled: false,
            ..TraceConfig::default()
        });
        engine.cfg.enabled = true;
        engine.job_tx = None;
        let target = target_v4();
        engine.running.insert(target);
        engine.cache.insert(
            target,
            TraceStatus::Running {
                previous: None,
                progress: TraceProgress {
                    target,
                    hops: Vec::new(),
                },
            },
        );
        let (sender, receiver) = mpsc::channel();
        engine.result_rx = receiver;
        sender
            .send(JobEvent::Finished {
                ip: target,
                generation: engine.generation,
                result: TraceResult {
                    target,
                    hops: vec![Hop {
                        ttl: 1,
                        addr: Some("192.0.2.1".parse().unwrap()),
                        rtt_ms: Some(1.0),
                    }],
                    finished_at: Instant::now(),
                    error: Some("timed out".into()),
                },
            })
            .unwrap();

        engine.poll();

        assert!(matches!(
            engine.get(target),
            TraceStatus::Done(result)
                if result.hops.len() == 1 && result.error.as_deref() == Some("timed out")
        ));
    }

    #[test]
    fn hop_updates_are_sorted_and_replace_duplicate_ttls() {
        let mut hops = Vec::new();
        upsert_hop(
            &mut hops,
            Hop {
                ttl: 2,
                addr: None,
                rtt_ms: None,
            },
        );
        upsert_hop(
            &mut hops,
            Hop {
                ttl: 1,
                addr: Some("192.0.2.1".parse().unwrap()),
                rtt_ms: Some(1.0),
            },
        );
        upsert_hop(
            &mut hops,
            Hop {
                ttl: 2,
                addr: Some("192.0.2.2".parse().unwrap()),
                rtt_ms: Some(2.0),
            },
        );

        assert_eq!(
            hops.iter().map(|hop| hop.ttl).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(hops[1].addr, Some("192.0.2.2".parse().unwrap()));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_uses_tracert() {
        let cmds = commands_for(target_v4(), &probe());
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].0, "tracert");
        assert!(cmds[0].1.iter().any(|a| a == "-d"));
        assert!(cmds[0].1.iter().any(|a| a == "-h"));
        assert!(!cmds[0].1.iter().any(|a| a == "-6"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_ipv6_adds_family_flag() {
        let v6: IpAddr = "2001:4860:4860::8888".parse().unwrap();
        let cmds = commands_for(v6, &probe());
        assert_eq!(cmds[0].0, "tracert");
        assert_eq!(cmds[0].1.first().map(String::as_str), Some("-6"));
    }
}
