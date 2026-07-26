use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::model::{
    AttributionSource, ConnState, Connection, ConnectionObservation, ProcessIdentity, SocketKey,
    UnattributedReason,
};

use super::events::{CollectionStatus, LifecycleEvents};
use super::{native, process};

const OBSERVED_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Default)]
pub enum NativeTrafficStatus {
    #[default]
    Disabled,
    Available,
    Unavailable(String),
}

#[derive(Debug, Clone, Default)]
pub enum UdpCollectionStatus {
    #[default]
    Disabled,
    Ready,
    Degraded(String),
    Unavailable(String),
}

#[derive(Debug, Clone)]
struct OwnerGroup {
    app_id: String,
    app_name: String,
    app_path: Option<std::path::PathBuf>,
    processes: Vec<ProcessIdentity>,
}

#[derive(Debug, Clone)]
struct ObservedSocket {
    owner: Option<OwnerGroup>,
    active: bool,
    last_seen: Instant,
    recovery_reported: bool,
    first_seen: Instant,
}

#[derive(Debug, Default)]
struct SessionCollectionStats {
    opens: u64,
    closes: u64,
    recovered_owners: u64,
    owner_gone: u64,
    ambiguous: u64,
    access_limited: u64,
}

#[derive(Debug)]
pub struct SocketCollector {
    observed: HashMap<SocketKey, ObservedSocket>,
    traffic_status: NativeTrafficStatus,
    udp_status: UdpCollectionStatus,
    events: LifecycleEvents,
    native_source: &'static str,
    udp_remote: bool,
    access_limited: usize,
    native_warnings: Vec<String>,
    session: SessionCollectionStats,
    topology_changed: bool,
}

impl Default for SocketCollector {
    fn default() -> Self {
        Self {
            observed: HashMap::new(),
            traffic_status: NativeTrafficStatus::Disabled,
            udp_status: UdpCollectionStatus::Disabled,
            events: LifecycleEvents::default(),
            native_source: if cfg!(target_os = "linux") {
                "linux-sock-diag"
            } else if cfg!(target_os = "macos") {
                "macos-libproc"
            } else {
                "windows-ip-helper"
            },
            udp_remote: cfg!(any(target_os = "linux", target_os = "macos")),
            access_limited: 0,
            native_warnings: Vec::new(),
            session: SessionCollectionStats::default(),
            topology_changed: false,
        }
    }
}

impl SocketCollector {
    fn attribute(
        &mut self,
        key: &SocketKey,
        mut owners: Vec<ProcessIdentity>,
        access_limited: bool,
    ) -> (
        Option<OwnerGroup>,
        AttributionSource,
        Option<UnattributedReason>,
        bool,
    ) {
        let previous = self.observed.get(key).cloned();
        let is_new = !previous.as_ref().is_some_and(|observed| observed.active);
        owners.sort_by(|a, b| a.id.cmp(&b.id));
        owners.dedup_by(|a, b| a.id == b.id);

        let mut app_ids: Vec<_> = owners.iter().map(|owner| owner.app_id.as_str()).collect();
        app_ids.sort_unstable();
        app_ids.dedup();
        let result = if app_ids.len() == 1 {
            let first = &owners[0];
            (
                Some(OwnerGroup {
                    app_id: first.app_id.clone(),
                    app_name: first.app_name.clone(),
                    app_path: first.app_path.clone(),
                    processes: owners,
                }),
                AttributionSource::Direct,
                None,
                is_new,
            )
        } else if app_ids.is_empty() {
            match previous.as_ref().and_then(|entry| entry.owner.clone()) {
                Some(owner) => (Some(owner), AttributionSource::Recovered, None, is_new),
                None => (
                    None,
                    AttributionSource::Unattributed,
                    Some(if access_limited {
                        UnattributedReason::AccessLimited
                    } else {
                        UnattributedReason::OwnerGone
                    }),
                    is_new,
                ),
            }
        } else {
            (
                None,
                AttributionSource::Unattributed,
                Some(UnattributedReason::Ambiguous),
                is_new,
            )
        };
        let recovery_reported = previous
            .as_ref()
            .is_some_and(|entry| entry.recovery_reported)
            || matches!(result.1, AttributionSource::Recovered);
        let first_seen = previous
            .as_ref()
            .filter(|entry| entry.active)
            .map(|entry| entry.first_seen)
            .unwrap_or_else(Instant::now);
        if matches!(result.1, AttributionSource::Recovered)
            && !previous
                .as_ref()
                .is_some_and(|entry| entry.recovery_reported)
        {
            self.session.recovered_owners = self.session.recovered_owners.saturating_add(1);
        }
        if is_new {
            self.session.opens = self.session.opens.saturating_add(1);
            match result.2 {
                Some(UnattributedReason::OwnerGone) => {
                    self.session.owner_gone = self.session.owner_gone.saturating_add(1)
                }
                Some(UnattributedReason::Ambiguous) => {
                    self.session.ambiguous = self.session.ambiguous.saturating_add(1)
                }
                Some(UnattributedReason::AccessLimited) => {
                    self.session.access_limited = self.session.access_limited.saturating_add(1)
                }
                None => {}
            }
        }
        self.observed.insert(
            key.clone(),
            ObservedSocket {
                owner: result.0.clone(),
                active: true,
                last_seen: Instant::now(),
                recovery_reported,
                first_seen,
            },
        );
        result
    }

    pub fn snapshot(
        &mut self,
        include_udp: bool,
        enhanced: bool,
    ) -> Result<Vec<ConnectionObservation>> {
        self.events.ensure_started();
        let previously_active: HashSet<_> = self
            .observed
            .iter()
            .filter(|(_, observed)| observed.active)
            .map(|(key, _)| key.clone())
            .collect();
        // Apply close events before reading the current socket table. If a
        // tuple was destroyed and immediately reused, attribution below then
        // treats the current socket as a new connection instead of silently
        // folding it into the old one.
        let mut out = self.drain_close_events();
        process::refresh();
        let snapshot = native::snapshot(include_udp, enhanced)
            .context("failed to read native system socket table")?;
        self.udp_status = if !include_udp {
            UdpCollectionStatus::Disabled
        } else if snapshot.udp_remote {
            if snapshot.access_limited == 0 {
                UdpCollectionStatus::Ready
            } else {
                UdpCollectionStatus::Degraded(format!(
                    "Connected UDP peers collected; {} protected process{} could not be inspected",
                    snapshot.access_limited,
                    if snapshot.access_limited == 1 {
                        ""
                    } else {
                        "es"
                    }
                ))
            }
        } else {
            UdpCollectionStatus::Unavailable(
                snapshot
                    .warnings
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "connected UDP collection is unavailable".into()),
            )
        };
        self.native_source = snapshot.source;
        self.udp_remote = snapshot.udp_remote;
        self.access_limited = snapshot.access_limited;
        self.native_warnings = snapshot.warnings.clone();
        self.traffic_status = if !enhanced {
            NativeTrafficStatus::Disabled
        } else if snapshot.traffic_counters {
            NativeTrafficStatus::Available
        } else {
            NativeTrafficStatus::Unavailable(
                "native TCP byte counters are unavailable for the active collector".into(),
            )
        };

        out.reserve(snapshot.sockets.len());
        for socket in snapshot.sockets {
            let key = socket.key();
            let owners = socket.pids.into_iter().map(process::resolve_info).collect();
            let (owner, attribution, reason, is_new) =
                self.attribute(&key, owners, self.access_limited > 0);
            let first_observed_at = self
                .observed
                .get(&key)
                .map(|entry| entry.first_seen)
                .unwrap_or_else(Instant::now);
            let processes = owner
                .as_ref()
                .map(|owner| owner.processes.clone())
                .unwrap_or_default();
            out.push(ConnectionObservation {
                active: true,
                connection: Connection {
                    application_id: owner.as_ref().map(|owner| owner.app_id.clone()),
                    pid: processes.first().map(|process| process.pid),
                    process_name: owner
                        .as_ref()
                        .map(|owner| owner.app_name.clone())
                        .unwrap_or_default(),
                    process_path: owner.as_ref().and_then(|owner| owner.app_path.clone()),
                    processes,
                    local: socket.local,
                    remote: socket.remote,
                    protocol: socket.protocol,
                    state: socket.state,
                    attribution,
                    unattributed_reason: reason,
                    is_new,
                    first_observed_at,
                    traffic_counters: socket.counters.map(|counters| {
                        crate::model::SocketTrafficCounters {
                            rx_bytes: counters.rx_bytes,
                            tx_bytes: counters.tx_bytes,
                        }
                    }),
                    destination_name: None,
                },
            });
        }

        let present: HashSet<_> = out
            .iter()
            .filter(|observation| observation.active)
            .map(|observation| observation.connection.socket_key())
            .collect();
        self.topology_changed = previously_active != present;
        self.session.closes = self
            .session
            .closes
            .saturating_add(previously_active.difference(&present).count() as u64);

        let now = Instant::now();
        for (key, observed) in &mut self.observed {
            if observed.active && !present.contains(key) {
                observed.active = false;
                observed.last_seen = now;
            }
        }
        self.observed.retain(|_, observed| {
            observed.active || now.duration_since(observed.last_seen) <= OBSERVED_TTL
        });
        Ok(out)
    }

    fn drain_close_events(&mut self) -> Vec<ConnectionObservation> {
        self.events
            .drain()
            .into_iter()
            .filter_map(|key| self.observe_close(key))
            .collect()
    }

    fn observe_close(&mut self, key: SocketKey) -> Option<ConnectionObservation> {
        let now = Instant::now();
        if let Some(observed) = self.observed.get_mut(&key) {
            if observed.active {
                observed.active = false;
                observed.last_seen = now;
            }
            return None;
        }
        self.observed.insert(
            key.clone(),
            ObservedSocket {
                owner: None,
                active: false,
                last_seen: now,
                recovery_reported: false,
                first_seen: now,
            },
        );
        self.session.owner_gone = self.session.owner_gone.saturating_add(1);
        Some(ConnectionObservation {
            active: false,
            connection: Connection {
                application_id: None,
                pid: None,
                process_name: String::new(),
                process_path: None,
                processes: Vec::new(),
                local: key.local,
                remote: key.remote,
                protocol: key.protocol,
                state: ConnState::Closed,
                attribution: AttributionSource::Unattributed,
                unattributed_reason: Some(UnattributedReason::OwnerGone),
                is_new: true,
                first_observed_at: now,
                traffic_counters: None,
                destination_name: None,
            },
        })
    }

    pub fn reset(&mut self) {
        self.observed.clear();
        self.events.clear();
        self.session = SessionCollectionStats::default();
        self.topology_changed = false;
    }

    pub fn traffic_status(&self) -> NativeTrafficStatus {
        self.traffic_status.clone()
    }

    pub fn udp_status(&self) -> UdpCollectionStatus {
        self.udp_status.clone()
    }

    pub fn collection_status(&self) -> CollectionStatus {
        let mut status = self.events.status();
        status.source = self.native_source;
        status.udp_remote = self.udp_remote;
        status.access_limited = self.access_limited;
        status.observed_opens = self.session.opens;
        status.observed_closes = self.session.closes;
        status.recovered_owners = self.session.recovered_owners;
        status.unattributed_owner_gone = self.session.owner_gone;
        status.unattributed_ambiguous = self.session.ambiguous;
        status.unattributed_access_limited = self.session.access_limited;
        if !self.native_warnings.is_empty() && status.status == "ready" {
            status.status = "degraded";
            status.message = self.native_warnings.join("; ");
        }
        status
    }

    pub fn take_topology_changed(&mut self) -> bool {
        std::mem::take(&mut self.topology_changed)
    }

    pub fn events_pending(&self) -> bool {
        self.events.has_pending()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn key(port: u16) -> SocketKey {
        SocketKey {
            protocol: crate::model::Protocol::Tcp,
            local: format!("127.0.0.1:{port}").parse().unwrap(),
            remote: "203.0.113.1:443".parse().unwrap(),
        }
    }

    fn owner(pid: u32, app: &str) -> ProcessIdentity {
        ProcessIdentity {
            id: format!("{pid}:1"),
            pid,
            start_time: 1,
            name: "helper".into(),
            path: Some(PathBuf::from(format!("/opt/{app}/helper"))),
            parent_pid: None,
            app_id: format!("/opt/{app}"),
            app_name: app.into(),
            app_path: Some(PathBuf::from(format!("/opt/{app}"))),
            is_app_root: false,
        }
    }

    #[test]
    fn multiple_helpers_in_one_app_are_attributed() {
        let mut collector = SocketCollector::default();
        let (group, source, reason, _) = collector.attribute(
            &key(41000),
            vec![owner(10, "browser"), owner(11, "browser")],
            false,
        );
        assert_eq!(source, AttributionSource::Direct);
        assert_eq!(reason, None);
        assert_eq!(group.unwrap().processes.len(), 2);
    }

    #[test]
    fn cross_app_owners_remain_ambiguous() {
        let mut collector = SocketCollector::default();
        let (_, source, reason, _) =
            collector.attribute(&key(41001), vec![owner(10, "one"), owner(11, "two")], false);
        assert_eq!(source, AttributionSource::Unattributed);
        assert_eq!(reason, Some(UnattributedReason::Ambiguous));
    }

    #[test]
    fn unseen_close_becomes_one_historical_observation() {
        let mut collector = SocketCollector::default();
        let socket = key(41002);
        assert!(collector
            .observe_close(socket.clone())
            .is_some_and(|observation| !observation.active));
        assert!(collector.observe_close(socket).is_none());
    }

    #[test]
    fn observed_close_is_deduplicated_and_tuple_reuse_is_new() {
        let mut collector = SocketCollector::default();
        let socket = key(41003);
        let (_, _, _, first_is_new) =
            collector.attribute(&socket, vec![owner(10, "browser")], false);
        assert!(first_is_new);
        assert!(collector.observe_close(socket.clone()).is_none());

        let (_, _, _, reused_is_new) =
            collector.attribute(&socket, vec![owner(10, "browser")], false);
        assert!(reused_is_new);
    }
}
