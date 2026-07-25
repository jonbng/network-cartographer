//! Representative desktop-shaped data for `--demo` and UI tests.

use crate::data::{Application, Density, Destination, Hop, Settings, Snapshot, TraceStats};

pub fn demo_snapshot() -> Snapshot {
    let mut snapshot = Snapshot {
        app_count: 4,
        destination_count: 5,
        live_connections: 14,
        missing_pid: 0,
        trace_stats: TraceStats {
            queued: 0,
            running: 1,
            done: 4,
            failed: 0,
        },
        geo_backend: "demo".into(),
        settings: Settings {
            external_only: true,
            traces_enabled: true,
            geo_local_only: false,
            history_enabled: false,
            privacy_accepted: true,
            density: Density::All,
        },
        demo: true,
        ..Snapshot::default()
    };

    snapshot.apps = vec![
        Application {
            id: "firefox".into(),
            name: "Firefox".into(),
            activity: 4.8,
            destinations: vec![
                destination(
                    "firefox|cloudflare|443",
                    "cdn.cloudflare.com",
                    "104.16.132.229",
                    443,
                    32.0,
                    vec![
                        hop(1, 18.47, -66.11, "San Juan", 2.0),
                        hop(5, 25.76, -80.19, "Miami", 12.0),
                        hop(9, 40.71, -74.01, "New York", 32.0),
                    ],
                ),
                destination(
                    "firefox|mozilla|443",
                    "services.mozilla.com",
                    "34.120.208.123",
                    443,
                    68.0,
                    vec![
                        hop(1, 18.47, -66.11, "San Juan", 2.0),
                        hop(5, 25.76, -80.19, "Miami", 13.0),
                        hop(11, 37.77, -122.42, "San Francisco", 68.0),
                    ],
                ),
            ],
        },
        Application {
            id: "spotify".into(),
            name: "Spotify".into(),
            activity: 2.1,
            destinations: vec![destination(
                "spotify|audio|443",
                "audio-fa.scdn.co",
                "35.186.224.25",
                443,
                91.0,
                vec![
                    hop(1, 18.47, -66.11, "San Juan", 2.0),
                    hop(6, 40.71, -74.01, "New York", 34.0),
                    hop(12, 53.35, -6.26, "Dublin", 91.0),
                ],
            )],
        },
        Application {
            id: "code".into(),
            name: "Code".into(),
            activity: 1.4,
            destinations: vec![destination(
                "code|github|443",
                "api.github.com",
                "140.82.114.6",
                443,
                47.0,
                vec![
                    hop(1, 18.47, -66.11, "San Juan", 2.0),
                    hop(5, 25.76, -80.19, "Miami", 12.0),
                    hop(10, 39.96, -83.00, "Columbus", 47.0),
                ],
            )],
        },
        Application {
            id: "slack".into(),
            name: "Slack".into(),
            activity: 0.6,
            destinations: vec![Destination {
                id: "slack|wss|443".into(),
                host: "wss-primary.slack.com".into(),
                ip: "34.120.54.55".into(),
                port: 443,
                protocol: "tcp".into(),
                hits: 4,
                org: Some("Google Cloud".into()),
                path_changed: false,
                status: "running".into(),
                rtt_ms: None,
                hops: Vec::new(),
            }],
        },
    ];
    snapshot
}

fn destination(
    id: &str,
    host: &str,
    ip: &str,
    port: u16,
    rtt_ms: f64,
    hops: Vec<Hop>,
) -> Destination {
    Destination {
        id: id.into(),
        host: host.into(),
        ip: ip.into(),
        port,
        protocol: "tcp".into(),
        hits: 12,
        org: None,
        path_changed: false,
        status: "done".into(),
        rtt_ms: Some(rtt_ms),
        hops,
    }
}

fn hop(ttl: u8, lat: f32, lon: f32, city: &str, _rtt_ms: f64) -> Hop {
    Hop {
        ttl,
        addr: None,
        lat: Some(lat),
        lon: Some(lon),
        city: Some(city.into()),
        country: None,
    }
}
