use std::{
    convert::Infallible,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode, Uri},
    middleware::{self, Next},
    response::{sse::Event, IntoResponse, Response, Sse},
    routing::{get, post},
    Json, Router,
};
use rust_embed::RustEmbed;
use serde::Serialize;
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};

use crate::{
    dto::{PathChangedEvent, SettingsDto, SnapshotDto},
    monitor::Monitor,
};

#[derive(RustEmbed)]
#[folder = "../dist/"]
struct WebAssets;

#[derive(Clone)]
struct AppState {
    monitor: Arc<Monitor>,
    events: broadcast::Sender<ServerEvent>,
    shutdown: broadcast::Sender<()>,
}

#[derive(Clone)]
enum ServerEvent {
    Snapshot(SnapshotDto),
    Error(String),
    PathChanged(PathChangedEvent),
}

#[derive(Serialize)]
struct VersionResponse {
    version: &'static str,
}

pub async fn run() -> Result<(), String> {
    let options = Options::from_env()?;
    let monitor = Arc::new(Monitor::new());
    let (events, _) = broadcast::channel(32);
    let (shutdown, _) = broadcast::channel(1);
    spawn_background_tasks(Arc::clone(&monitor), events.clone());

    let state = AppState {
        monitor,
        events,
        shutdown: shutdown.clone(),
    };
    let app = Router::new()
        .route("/api/version", get(version))
        .route("/api/snapshot", get(snapshot))
        .route("/api/refresh", post(refresh))
        .route("/api/settings", get(settings).put(update_settings))
        .route("/api/reset", post(reset))
        .route("/api/trace-all", post(trace_all))
        .route("/api/events", get(event_stream))
        .fallback(get(static_asset))
        .with_state(state)
        .layer(middleware::from_fn(validate_host));

    let address = SocketAddr::from(([127, 0, 0, 1], options.port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| format!("could not listen on http://{address}: {error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("could not read server address: {error}"))?;
    let url = format!("http://{address}");

    println!("Network Cartographer is running at {url}");
    println!("Press Ctrl+C to stop.");
    if options.open_browser {
        if let Err(error) = webbrowser::open(&url) {
            eprintln!("Could not open your browser: {error}");
            eprintln!("Open {url} manually.");
        }
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown))
        .await
        .map_err(|error| format!("web server stopped unexpectedly: {error}"))
}

async fn shutdown_signal(shutdown: broadcast::Sender<()>) {
    let _ = tokio::signal::ctrl_c().await;
    println!("\nStopping Network Cartographer…");
    // Graceful shutdown waits for every response body to finish. Tell the
    // long-lived SSE response to close before asking Axum to drain connections.
    let _ = shutdown.send(());
}

async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn snapshot(State(state): State<AppState>) -> Json<SnapshotDto> {
    Json(state.monitor.snapshot())
}

async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SnapshotDto>, (StatusCode, String)> {
    require_local_action(&headers)?;
    state.monitor.tick().map(Json).map_err(internal_error)
}

async fn settings(State(state): State<AppState>) -> Json<SettingsDto> {
    Json(state.monitor.settings.lock().clone())
}

async fn update_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(settings): Json<SettingsDto>,
) -> Result<Json<SettingsDto>, (StatusCode, String)> {
    require_local_action(&headers)?;
    state.monitor.apply_settings(settings);
    Ok(Json(state.monitor.settings.lock().clone()))
}

async fn reset(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, String)> {
    require_local_action(&headers)?;
    state.monitor.reset();
    Ok(StatusCode::NO_CONTENT)
}

async fn trace_all(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, String)> {
    require_local_action(&headers)?;
    let ips = state.monitor.state.lock().unique_remote_ips();
    state.monitor.traces.lock().force_many(ips);
    Ok(StatusCode::ACCEPTED)
}

async fn event_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut shutdown = state.shutdown.subscribe();
    let stream = BroadcastStream::new(state.events.subscribe()).filter_map(|message| {
        let event = match message.ok()? {
            ServerEvent::Snapshot(snapshot) => event("monitor-update", &snapshot),
            ServerEvent::Error(error) => event("monitor-error", &error),
            ServerEvent::PathChanged(change) => event("path-changed", &change),
        };
        Some(Ok(event))
    });
    let stream = futures_util::StreamExt::take_until(stream, async move {
        let _ = shutdown.recv().await;
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

fn event<T: Serialize>(name: &'static str, payload: &T) -> Event {
    let data = serde_json::to_string(payload).unwrap_or_else(|_| "null".into());
    Event::default().event(name).data(data)
}

fn internal_error(error: String) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error)
}

fn require_local_action(headers: &HeaderMap) -> Result<(), (StatusCode, String)> {
    if headers
        .get("X-Network-Cartographer")
        .and_then(|value| value.to_str().ok())
        == Some("1")
    {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "missing local action header".into()))
    }
}

async fn validate_host(request: Request, next: Next) -> Response {
    let allowed = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(':').next())
        .is_some_and(|host| host == "127.0.0.1" || host == "localhost");

    if allowed {
        next.run(request).await
    } else {
        (StatusCode::FORBIDDEN, "invalid local host").into_response()
    }
}

async fn static_asset(uri: Uri) -> Response {
    let requested = uri.path().trim_start_matches('/');
    let path = if requested.is_empty() {
        "index.html"
    } else {
        requested
    };

    if let Some(asset) = WebAssets::get(path) {
        return asset_response(path, asset.data.into_owned());
    }

    // The frontend currently has no client-side routes, but falling back to
    // the shell keeps direct navigation working if routes are added later.
    if let Some(index) = WebAssets::get("index.html") {
        return asset_response("index.html", index.data.into_owned());
    }

    (StatusCode::NOT_FOUND, "UI assets were not embedded").into_response()
}

fn asset_response(path: &str, bytes: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache = if path == "index.html" {
        "no-cache"
    } else {
        "public, max-age=31536000, immutable"
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, cache)
        .header("X-Content-Type-Options", "nosniff")
        .header(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'self'; img-src 'self' blob: data:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self'; worker-src 'self' blob:; font-src 'self' data:; frame-ancestors 'none'; base-uri 'none'",
        )
        .header("Referrer-Policy", "no-referrer")
        .body(Body::from(bytes))
        .unwrap()
}

fn spawn_background_tasks(monitor: Arc<Monitor>, events: broadcast::Sender<ServerEvent>) {
    {
        let monitor = Arc::clone(&monitor);
        thread::Builder::new()
            .name("geo-warm".into())
            .spawn(move || loop {
                let settings = monitor.settings.lock().clone();
                if !settings.privacy_accepted {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                let pending = monitor.pending_geo_ips();
                if pending.is_empty() {
                    thread::sleep(Duration::from_secs(2));
                    continue;
                }
                let batch: Vec<IpAddr> = pending.into_iter().take(40).collect();
                monitor.geo.resolve_batch(&batch, settings.geo_local_only);
                monitor.path_geo.clear();
                thread::sleep(Duration::from_millis(600));
            })
            .expect("spawn geo warmer");
    }

    {
        let monitor = Arc::clone(&monitor);
        let events = events.clone();
        thread::Builder::new()
            .name("history".into())
            .spawn(move || loop {
                thread::sleep(Duration::from_secs(60));
                if !monitor.settings.lock().history_enabled {
                    continue;
                }
                if let Err(error) = crate::history::append_snapshot(&monitor.snapshot()) {
                    let _ = events.send(ServerEvent::Error(format!("history: {error}")));
                }
            })
            .expect("spawn history writer");
    }

    thread::Builder::new()
        .name("monitor-poll".into())
        .spawn(move || {
            let mut last_socket_poll = Instant::now() - Duration::from_secs(10);
            let mut last_geo_emit = Instant::now() - Duration::from_secs(10);
            let mut last_pending_geo = 0usize;

            loop {
                let interval =
                    Duration::from_millis(monitor.settings.lock().poll_interval_ms.max(500));
                if last_socket_poll.elapsed() >= interval {
                    match monitor.tick() {
                        Ok(snapshot) => {
                            let _ = events.send(ServerEvent::Snapshot(snapshot));
                            for change in monitor.drain_path_change_events() {
                                let _ = events.send(ServerEvent::PathChanged(change));
                            }
                        }
                        Err(error) => {
                            let _ = events.send(ServerEvent::Error(error));
                        }
                    }
                    last_socket_poll = Instant::now();
                    last_geo_emit = Instant::now();
                    last_pending_geo = monitor.pending_geo_ips().len();
                } else {
                    let pending = monitor.pending_geo_ips().len();
                    let geo_progressed = pending < last_pending_geo;
                    if (geo_progressed || pending > 0)
                        && last_geo_emit.elapsed() >= Duration::from_millis(1500)
                    {
                        let _ = events.send(ServerEvent::Snapshot(monitor.snapshot()));
                        last_geo_emit = Instant::now();
                        last_pending_geo = pending;
                    }
                }
                thread::sleep(Duration::from_millis(350));
            }
        })
        .expect("spawn monitor poll loop");
}

struct Options {
    port: u16,
    open_browser: bool,
}

impl Options {
    fn from_env() -> Result<Self, String> {
        let mut port = 4769;
        let mut open_browser = true;
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--no-open" => open_browser = false,
                "--port" => {
                    let value = args.next().ok_or("--port requires a number")?;
                    port = value
                        .parse::<u16>()
                        .map_err(|_| format!("invalid port: {value}"))?;
                }
                "-h" | "--help" => {
                    println!(
                        "Network Cartographer\n\nUSAGE:\n  netcart [--port PORT] [--no-open]\n\nOPTIONS:\n  --port PORT  Local port (default: 4769)\n  --no-open    Do not open the browser automatically\n  -h, --help   Show this help"
                    );
                    std::process::exit(0);
                }
                unknown => return Err(format!("unknown option: {unknown}")),
            }
        }

        Ok(Self { port, open_browser })
    }
}
