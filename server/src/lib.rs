mod collect;
mod dto;
mod geo;
mod history;
mod model;
mod monitor;
mod network_origin;
mod resolve;
mod server;
mod settings_store;
mod trace;

/// Direct access to the same monitor and DTOs used by the browser UI.
///
/// This keeps alternate frontends (currently the terminal experiment) on the
/// main product's data model without exposing the monitor's internal locks.
pub mod standalone {
    use std::{net::IpAddr, sync::Arc};

    pub use crate::dto::{
        AppDto, AttributionStatsDto, CollectionDto, DestDto, DestinationNamingDto, HopDto,
        MonitoringDto, NetworkExitDto, NetworkOriginDto, NetworkOriginEvidenceDto, ProcessDto,
        SettingsDto, SnapshotDto, TraceDto, TraceStatsDto, TrafficGroupDto, TrafficRateDto,
        UdpMonitoringDto,
    };

    use crate::monitor::Monitor;

    #[derive(Clone)]
    pub struct StandaloneMonitor {
        inner: Arc<Monitor>,
    }

    impl Default for StandaloneMonitor {
        fn default() -> Self {
            Self::new()
        }
    }

    impl StandaloneMonitor {
        pub fn new() -> Self {
            Self {
                inner: Arc::new(Monitor::new()),
            }
        }

        pub fn snapshot(&self) -> SnapshotDto {
            self.inner.snapshot()
        }

        pub fn tick(&self) -> Result<SnapshotDto, String> {
            self.inner.tick()
        }

        pub fn settings(&self) -> SettingsDto {
            self.inner.settings.lock().clone()
        }

        pub fn apply_settings(&self, settings: SettingsDto) -> SettingsDto {
            self.inner.apply_settings(settings);
            self.settings()
        }

        pub fn force_trace_all(&self) {
            let ips = self.inner.state.lock().unique_remote_ips();
            self.inner.traces.lock().force_many(ips);
        }

        pub fn reset(&self) {
            self.inner.reset();
        }

        pub fn append_history(&self) -> Result<(), String> {
            crate::history::append_snapshot(&self.snapshot())
        }

        /// Resolve one bounded batch of pending hop locations. Callers run
        /// this away from their render/input thread because online GeoIP can
        /// block on the network.
        pub fn warm_geo_once(&self, limit: usize) -> usize {
            let settings = self.settings();
            let pending = self.inner.pending_geo_ips();
            let batch: Vec<IpAddr> = pending.into_iter().take(limit.max(1)).collect();
            if batch.is_empty() {
                return 0;
            }
            self.inner
                .geo
                .resolve_batch(&batch, settings.geo_local_only);
            self.inner.path_geo.clear();
            batch.len()
        }
    }
}

pub async fn run() -> Result<(), String> {
    server::run().await
}
