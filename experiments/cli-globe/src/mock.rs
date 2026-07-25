//! Demo paths so the experiment runs without the Tauri backend.

use crate::globe::{Hop, Path};

pub fn demo_paths() -> Vec<Path> {
    vec![
        Path {
            app: "firefox".into(),
            host: "cdn.cloudflare.com".into(),
            color: [34, 211, 238],
            hops: vec![
                Hop {
                    lat: 59.33,
                    lon: 18.07,
                    label: "you (Stockholm)".into(),
                },
                Hop {
                    lat: 59.2,
                    lon: 17.9,
                    label: "isp".into(),
                },
                Hop {
                    lat: 52.52,
                    lon: 13.40,
                    label: "berlin".into(),
                },
                Hop {
                    lat: 50.11,
                    lon: 8.68,
                    label: "frankfurt".into(),
                },
                Hop {
                    lat: 51.50,
                    lon: -0.12,
                    label: "london edge".into(),
                },
            ],
        },
        Path {
            app: "spotify".into(),
            host: "audio-fa.scdn.co".into(),
            color: [167, 139, 250],
            hops: vec![
                Hop {
                    lat: 59.33,
                    lon: 18.07,
                    label: "you".into(),
                },
                Hop {
                    lat: 53.35,
                    lon: -6.26,
                    label: "dublin".into(),
                },
                Hop {
                    lat: 40.71,
                    lon: -74.01,
                    label: "nyc".into(),
                },
            ],
        },
        Path {
            app: "code".into(),
            host: "api.github.com".into(),
            color: [74, 222, 128],
            hops: vec![
                Hop {
                    lat: 59.33,
                    lon: 18.07,
                    label: "you".into(),
                },
                Hop {
                    lat: 50.11,
                    lon: 8.68,
                    label: "fra".into(),
                },
                Hop {
                    lat: 37.77,
                    lon: -122.42,
                    label: "sfo".into(),
                },
            ],
        },
        Path {
            app: "slack".into(),
            host: "wss-primary.slack.com".into(),
            color: [251, 146, 60],
            hops: vec![
                Hop {
                    lat: 59.33,
                    lon: 18.07,
                    label: "you".into(),
                },
                Hop {
                    lat: 48.85,
                    lon: 2.35,
                    label: "paris".into(),
                },
                Hop {
                    lat: 37.39,
                    lon: -122.08,
                    label: "sjc".into(),
                },
            ],
        },
    ]
}
