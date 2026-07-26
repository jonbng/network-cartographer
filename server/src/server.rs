use std::{
    convert::Infallible,
    fs,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path as AxumPath, Request, State},
    http::{header, HeaderMap, StatusCode, Uri},
    middleware::{self, Next},
    response::{sse::Event, IntoResponse, Response, Sse},
    routing::{get, post},
    Json, Router,
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};

use crate::{
    app_icons::icon_id_from_request,
    collect::SniObservation,
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
    observation_token: Arc<str>,
}

#[derive(Clone)]
enum ServerEvent {
    Snapshot(Box<SnapshotDto>),
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
    let observation_token: Arc<str> = generate_observation_token()?.into();
    spawn_background_tasks(Arc::clone(&monitor), events.clone());

    let state = AppState {
        monitor,
        events,
        shutdown: shutdown.clone(),
        observation_token: Arc::clone(&observation_token),
    };
    let app = Router::new()
        .route("/api/version", get(version))
        .route("/api/snapshot", get(snapshot))
        .route("/api/refresh", post(refresh))
        .route("/api/settings", get(settings).put(update_settings))
        .route("/api/reset", post(reset))
        .route("/api/trace/{ip}", post(trace_one))
        .route(
            "/api/observations/sni",
            post(record_sni).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route("/api/events", get(event_stream))
        .route("/api/app-icons/{id}", get(app_icon))
        .fallback(get(static_asset))
        .with_state(state)
        .layer(middleware::from_fn(validate_host));

    let (listener, used_fallback_port) = bind_listener(&options).await?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("could not read server address: {error}"))?;
    report_successful_startup();
    let url = format!("http://{address}");
    let _feed_discovery = match FeedDiscovery::write(&url, &observation_token) {
        Ok(discovery) => Some(discovery),
        Err(error) => {
            eprintln!("  Domains    SNI feed discovery unavailable ({error})");
            None
        }
    };

    if std::env::var_os("NETCART_LAUNCHED").is_none() {
        println!("Network Cartographer");
        println!("---------------------");
    }
    println!("  Status     Running");
    println!("  Dashboard  {url}");
    if used_fallback_port {
        println!("  Port       4769 was busy; using {}", address.port());
    }
    println!("  Access     This machine only");
    if options.open_browser {
        if let Err(error) = webbrowser::open(&url) {
            eprintln!("  Browser    Could not open automatically");
            eprintln!("             Open {url} manually ({error})");
        } else {
            println!("  Browser    Opened automatically");
        }
    } else {
        println!("  Browser    Not opened (--no-open)");
    }
    println!("  Stop       Press Ctrl+C");
    println!();

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown))
        .await
        .map_err(|error| format!("web server stopped unexpectedly: {error}"))?;
    println!("  Status     Stopped");
    Ok(())
}

async fn shutdown_signal(shutdown: broadcast::Sender<()>) {
    let _ = tokio::signal::ctrl_c().await;
    println!("\n  Status     Stopping…");
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

async fn app_icon(State(state): State<AppState>, AxumPath(request): AxumPath<String>) -> Response {
    let Some(id) = icon_id_from_request(&request) else {
        return (StatusCode::NOT_FOUND, "app icon not found").into_response();
    };
    let monitor = Arc::clone(&state.monitor);
    let id = id.to_owned();
    let bytes = tokio::task::spawn_blocking(move || monitor.app_icons.get(&id))
        .await
        .ok()
        .flatten();
    let Some(bytes) = bytes else {
        return (StatusCode::NOT_FOUND, "app icon not found").into_response();
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CACHE_CONTROL, "private, max-age=3600, immutable")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(bytes.to_vec()))
        .unwrap()
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

async fn trace_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(ip): AxumPath<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    require_local_action(&headers)?;
    let ip = ip
        .parse::<IpAddr>()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid destination IP".into()))?;
    state.monitor.traces.lock().force(ip);
    Ok(StatusCode::ACCEPTED)
}

async fn record_sni(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(observation): Json<SniObservation>,
) -> Result<StatusCode, (StatusCode, String)> {
    let expected = format!("Bearer {}", state.observation_token);
    if headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        != Some(expected.as_str())
    {
        return Err((StatusCode::UNAUTHORIZED, "invalid observation token".into()));
    }
    state.monitor.record_sni(observation).map_err(|error| {
        let status = if error.contains("disabled") {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_REQUEST
        };
        (status, error)
    })?;
    Ok(StatusCode::NO_CONTENT)
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
            .name("domain-observer".into())
            .spawn(move || loop {
                monitor.poll_domain_sources();
                thread::sleep(Duration::from_secs(2));
            })
            .expect("spawn destination domain observer");
    }

    {
        let monitor = Arc::clone(&monitor);
        thread::Builder::new()
            .name("geo-warm".into())
            .spawn(move || loop {
                let settings = monitor.settings.lock().clone();
                let pending = monitor.pending_geo_ips();
                if pending.is_empty() {
                    thread::sleep(Duration::from_millis(250));
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
        thread::Builder::new()
            .name("network-origin".into())
            .spawn(move || {
                let mut next_hosted = Instant::now();
                let mut retry = Duration::from_secs(30);
                let poll_interval = if cfg!(target_os = "macos") {
                    Duration::from_secs(2)
                } else {
                    Duration::from_secs(10)
                };
                loop {
                    let route_changed = monitor.network_origin.refresh_local();
                    let route_change_pending = monitor.network_origin.change_candidate_pending();
                    let local_only = monitor.settings.lock().geo_local_only;
                    let now = Instant::now();
                    if route_changed {
                        monitor.handle_network_change();
                    }
                    if !local_only && !route_change_pending && (route_changed || now >= next_hosted)
                    {
                        match monitor.network_origin.refresh_hosted() {
                            Ok(()) => {
                                retry = Duration::from_secs(30);
                                next_hosted = Instant::now() + Duration::from_secs(300);
                            }
                            Err(_) => {
                                next_hosted = Instant::now() + retry;
                                retry = (retry * 2).min(Duration::from_secs(300));
                            }
                        }
                    } else if local_only {
                        if route_changed {
                            monitor.network_origin.complete_local_transition();
                        }
                        next_hosted = now;
                        retry = Duration::from_secs(30);
                    }
                    thread::sleep(poll_interval);
                }
            })
            .expect("spawn network origin observer");
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
            let mut last_snapshot_emit = Instant::now() - Duration::from_secs(10);
            let mut last_pending_geo = 0usize;
            let mut traces_were_pending = false;
            let mut trace_completion_due = false;
            let mut cadence = AdaptiveCadence::new(Instant::now());

            loop {
                let now = Instant::now();
                let idle_interval = Duration::from_millis(
                    monitor.settings.lock().poll_interval_ms.clamp(500, 2_000),
                );
                let (phase, interval) = cadence.target(now, idle_interval);
                let event_ready = monitor.collection_events_pending()
                    && last_socket_poll.elapsed() >= Duration::from_millis(50);
                if last_socket_poll.elapsed() >= interval || event_ready {
                    let effective_interval = last_socket_poll.elapsed();
                    monitor.set_capture_runtime(
                        phase,
                        effective_interval.as_millis().min(u128::from(u64::MAX)) as u64,
                    );
                    match monitor.tick() {
                        Ok(snapshot) => {
                            let traces_pending =
                                snapshot.trace_stats.queued + snapshot.trace_stats.running > 0;
                            if traces_were_pending && !traces_pending {
                                trace_completion_due = true;
                            }
                            traces_were_pending = traces_pending;
                            let changed = monitor.take_collection_changed();
                            if changed {
                                cadence.note_change(Instant::now());
                            }
                            if changed || last_snapshot_emit.elapsed() >= Duration::from_secs(1) {
                                let _ = events.send(ServerEvent::Snapshot(Box::new(snapshot)));
                                last_snapshot_emit = Instant::now();
                                trace_completion_due = false;
                            }
                            for change in monitor.drain_path_change_events() {
                                let _ = events.send(ServerEvent::PathChanged(change));
                            }
                        }
                        Err(error) => {
                            let _ = events.send(ServerEvent::Error(error));
                            let _ =
                                events.send(ServerEvent::Snapshot(Box::new(monitor.snapshot())));
                        }
                    }
                    last_socket_poll = Instant::now();
                    last_pending_geo = monitor.pending_geo_ips().len();
                } else {
                    let traces_pending = monitor.traces_pending();
                    if traces_were_pending && !traces_pending {
                        trace_completion_due = true;
                    }
                    traces_were_pending = traces_pending;
                    let pending = monitor.pending_geo_ips().len();
                    let geo_progressed = pending < last_pending_geo;
                    let trace_update_due = (traces_pending || trace_completion_due)
                        && last_snapshot_emit.elapsed() >= Duration::from_secs(1);
                    let geo_update_due = (geo_progressed || pending > 0)
                        && last_snapshot_emit.elapsed() >= Duration::from_millis(1500);
                    if trace_update_due || geo_update_due {
                        let _ = events.send(ServerEvent::Snapshot(Box::new(monitor.snapshot())));
                        last_snapshot_emit = Instant::now();
                        last_pending_geo = pending;
                        trace_completion_due = false;
                    }
                }
                thread::sleep(Duration::from_millis(50));
            }
        })
        .expect("spawn monitor poll loop");
}

const DEFAULT_RUNS_URL: &str = "https://mapmy.network/api/v1/runs";

fn report_successful_startup() {
    if cfg!(debug_assertions) || std::env::var_os("NETCART_DISABLE_USAGE_PING").is_some() {
        return;
    }

    let url = std::env::var("NETWORK_CARTOGRAPHER_RUNS_URL")
        .unwrap_or_else(|_| DEFAULT_RUNS_URL.to_string());
    let _ = thread::Builder::new()
        .name("usage-ping".into())
        .spawn(move || {
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(2))
                .timeout_read(Duration::from_secs(2))
                .user_agent(concat!(
                    "netcart/",
                    env!("CARGO_PKG_VERSION"),
                    " (+https://mapmy.network)"
                ))
                .build();
            // Best effort only: startup and shutdown never depend on telemetry.
            let _ = agent.post(&url).call();
        });
}

struct AdaptiveCadence {
    last_change: Instant,
}

impl AdaptiveCadence {
    fn new(now: Instant) -> Self {
        Self { last_change: now }
    }

    fn note_change(&mut self, now: Instant) {
        self.last_change = now;
    }

    fn target(&self, now: Instant, idle: Duration) -> (&'static str, Duration) {
        let quiet_for = now.saturating_duration_since(self.last_change);
        if quiet_for < Duration::from_secs(10) {
            ("active", Duration::from_millis(250))
        } else if quiet_for < Duration::from_secs(30) {
            ("warm", Duration::from_millis(500))
        } else {
            ("idle", idle)
        }
    }
}

fn generate_observation_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|error| error.to_string())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeedDiscoveryDocument {
    pid: u32,
    endpoint: String,
    token: String,
}

struct FeedDiscovery {
    path: PathBuf,
}

impl FeedDiscovery {
    fn write(base_url: &str, token: &str) -> Result<Self, String> {
        let directory = dirs::config_dir()
            .ok_or_else(|| "no config directory for SNI feed discovery".to_string())?
            .join("network-cartographer")
            .join("runtime");
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        remove_stale_feed_files(&directory);
        let path = directory.join(format!("observation-feed-{}.json", std::process::id()));
        let document = FeedDiscoveryDocument {
            pid: std::process::id(),
            endpoint: format!("{base_url}/api/observations/sni"),
            token: token.into(),
        };
        let bytes = serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?;

        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(&path)
                .map_err(|error| error.to_string())?;
            file.write_all(&bytes).map_err(|error| error.to_string())?;
        }
        #[cfg(not(unix))]
        fs::write(&path, bytes).map_err(|error| error.to_string())?;

        Ok(Self { path })
    }
}

fn remove_stale_feed_files(directory: &std::path::Path) {
    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_feed = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("observation-feed-") && name.ends_with(".json"));
        if !is_feed {
            continue;
        }
        let document = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<FeedDiscoveryDocument>(&bytes).ok());
        let stale = document.is_none_or(|document| {
            system
                .process(sysinfo::Pid::from_u32(document.pid))
                .is_none()
        });
        if stale {
            let _ = fs::remove_file(path);
        }
    }
}

impl Drop for FeedDiscovery {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

struct Options {
    port: u16,
    port_explicit: bool,
    open_browser: bool,
}

async fn bind_listener(options: &Options) -> Result<(tokio::net::TcpListener, bool), String> {
    let address = SocketAddr::from(([127, 0, 0, 1], options.port));
    match tokio::net::TcpListener::bind(address).await {
        Ok(listener) => Ok((listener, false)),
        Err(error) if !options.port_explicit && error.kind() == std::io::ErrorKind::AddrInUse => {
            let fallback = SocketAddr::from(([127, 0, 0, 1], 0));
            tokio::net::TcpListener::bind(fallback)
                .await
                .map(|listener| (listener, true))
                .map_err(|fallback_error| {
                    format!(
                        "could not listen on http://{address} or another local port: {fallback_error}"
                    )
                })
        }
        Err(error) => Err(format!("could not listen on http://{address}: {error}")),
    }
}

impl Options {
    fn from_env() -> Result<Self, String> {
        let mut port = 4769;
        let mut port_explicit = false;
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
                    port_explicit = true;
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

        Ok(Self {
            port,
            port_explicit,
            open_browser,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn occupied_default_port_falls_back_to_loopback_ephemeral_port() {
        let occupied = match std::net::TcpListener::bind(("127.0.0.1", 0)) {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
            Err(error) => panic!("could not reserve test port: {error}"),
        };
        let port = occupied.local_addr().unwrap().port();
        let options = Options {
            port,
            port_explicit: false,
            open_browser: false,
        };

        let (listener, fallback) = bind_listener(&options).await.unwrap();
        assert!(fallback);
        assert_ne!(listener.local_addr().unwrap().port(), port);
    }

    #[tokio::test]
    async fn occupied_explicit_port_is_an_error() {
        let occupied = match std::net::TcpListener::bind(("127.0.0.1", 0)) {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
            Err(error) => panic!("could not reserve test port: {error}"),
        };
        let options = Options {
            port: occupied.local_addr().unwrap().port(),
            port_explicit: true,
            open_browser: false,
        };

        assert!(bind_listener(&options).await.is_err());
    }

    fn test_state(token: &str) -> AppState {
        let (events, _) = broadcast::channel(2);
        let (shutdown, _) = broadcast::channel(1);
        AppState {
            monitor: Arc::new(Monitor::new()),
            events,
            shutdown,
            observation_token: Arc::from(token),
        }
    }

    fn observation() -> SniObservation {
        SniObservation {
            hostname: "www.example.com".into(),
            remote_ip: "203.0.113.10".parse().unwrap(),
            remote_port: Some(443),
            pid: None,
            local_ip: None,
            local_port: None,
        }
    }

    #[tokio::test]
    async fn sni_feed_requires_bearer_token() {
        let result = record_sni(
            State(test_state("secret")),
            HeaderMap::new(),
            Json(observation()),
        )
        .await;
        assert_eq!(result.unwrap_err().0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn sni_feed_accepts_valid_observation() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer secret".parse().unwrap());
        let result = record_sni(State(test_state("secret")), headers, Json(observation())).await;
        assert_eq!(result.unwrap(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn app_icon_route_rejects_non_opaque_ids() {
        let response = app_icon(
            State(test_state("secret")),
            AxumPath("../../Applications/Safari.app".into()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn adaptive_cadence_moves_from_active_to_warm_to_idle() {
        let start = Instant::now();
        let mut cadence = AdaptiveCadence::new(start);
        assert_eq!(
            cadence.target(start + Duration::from_secs(9), Duration::from_secs(1)),
            ("active", Duration::from_millis(250))
        );
        assert_eq!(
            cadence.target(start + Duration::from_secs(10), Duration::from_secs(1)),
            ("warm", Duration::from_millis(500))
        );
        assert_eq!(
            cadence.target(start + Duration::from_secs(30), Duration::from_secs(1)),
            ("idle", Duration::from_secs(1))
        );
        cadence.note_change(start + Duration::from_secs(31));
        assert_eq!(
            cadence.target(start + Duration::from_secs(31), Duration::from_secs(1)),
            ("active", Duration::from_millis(250))
        );
    }
}
