# Map My Network

A local command-line network monitor with a browser-based 3D globe. It shows which applications are talking to the internet, where they connect, and the routes traffic takes to each destination.

The command binds only to `127.0.0.1`, opens your default browser, and stops when you press Ctrl+C. There is no desktop WebView, tray process, installer, remote Map My Network monitoring service, or privileged runtime mode.

[![CI](https://github.com/jonbng/network-cartographer/actions/workflows/ci.yml/badge.svg)](https://github.com/jonbng/network-cartographer/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)

## Try it now

No clone, Node.js, Rust, installer, or account is required. On macOS or Linux:

```bash
curl -fsSL https://github.com/jonbng/network-cartographer/releases/latest/download/run.sh | sh
```

On Windows, paste this into PowerShell:

```powershell
irm https://github.com/jonbng/network-cartographer/releases/latest/download/run.ps1 | iex
```

The launcher downloads the latest release to a temporary directory, runs it as your current user, opens the UI, and removes the binary when you stop it with Ctrl+C. The scripts and release binaries are public and auditable in this repository.

## Features

- Live per-application TCP connection monitoring
- Destination IP, hostname, port, organization, and unique connection activity
- Optional per-application upload/download rates on Linux via kernel socket diagnostics
- Background unprivileged traceroutes with bounded concurrency
- Hop geolocation with local MaxMind and optional online fallbacks
- Interactive Three.js / globe.gl visualization
- Application focus, search, route history, and density controls
- All monitoring data stays in the local CLI process

## Run from source

Requirements:

- Rust 1.88+
- Node.js 18+
- A modern browser
- Optional: the platform's `traceroute`, `tracepath`, or `tracert` command

```bash
git clone https://github.com/jonbng/network-cartographer.git
cd network-cartographer
npm install
npm start
```

`npm start` builds the UI, starts the local server, and opens the browser. Stop it with Ctrl+C.

## Install the CLI locally

Build the frontend once, then install the Rust command into your Cargo bin directory:

```bash
npm install
npm run build
cargo install --path server
netcart
```

This is a user-local Cargo installation and does not need a system installer.

Prebuilt release archives contain a single `netcart` binary with the UI embedded. Releases currently cover Linux x64/ARM64, macOS Intel/Apple Silicon, and Windows x64.

## Command options

```text
netcart [--port PORT] [--no-open]

--port PORT  Local port (default: 4769)
--no-open    Start the server without opening a browser
```

The server deliberately listens only on loopback. It is not intended to be exposed to a LAN or the public internet.

## Development

Run the complete embedded-UI application:

```bash
npm start
```

For frontend hot reload, run the backend and Vite in separate terminals:

```bash
cargo run --manifest-path server/Cargo.toml -- --no-open
npm run frontend:dev
```

Vite proxies `/api` to the local server on port 4769.

Checks:

```bash
npm run check
npm test
```

Release build:

```bash
npm run build
cargo build --release --locked --manifest-path server/Cargo.toml
```

The executable is written to `server/target/release/`.

## Architecture

```text
ui/                 Vite + TypeScript + globe.gl browser UI
server/             Rust CLI, local HTTP API, and embedded UI assets
  src/collect/      OS socket tables → process map
  src/model/        Aggregation by application and destination
  src/resolve/      Reverse DNS workers
  src/trace/        Unprivileged traceroute queue and parsers
  src/geo/          Hop geolocation and confidence refinement
  src/monitor.rs    Connection monitor and snapshot generation
  src/server.rs     Loopback HTTP/SSE server and browser launcher
experiments/        Unsupported interface experiments
```

The browser reads snapshots through a same-origin localhost API. Live updates use Server-Sent Events. Mutating requests require a non-simple local action header, and no cross-origin access is enabled.

## Traceroute behavior

Map My Network never changes users or requests extra permissions. It uses normal-user probe modes only:

| OS | Probe method |
|----|--------------|
| Linux | UDP `traceroute`, then `tracepath` fallback |
| macOS / BSD | UDP `traceroute` |
| Windows | Built-in `tracert` |

If no supported command is installed, connection monitoring still works and route entries report the traceroute error.

## Geolocation

Geolocation is approximate. The app combines reverse DNS hints, airport codes, GeoIP, latency consistency, and neighboring hops.

For fully local lookups, provide MaxMind databases with either environment variables or standard data directories:

| Variable | Purpose |
|----------|---------|
| `NETWORK_CARTOGRAPHER_MMDB` | Absolute path to GeoLite2-City.mmdb |
| `NETWORK_CARTOGRAPHER_ASN_MMDB` | Absolute path to GeoLite2-ASN.mmdb |

Common locations include the project root, `data/`, the directory beside the binary, `~/.local/share/GeoIP/`, `~/Library/Application Support/GeoIP/`, and `%LOCALAPPDATA%\GeoIP\`.

Do not commit MaxMind databases; they are ignored by Git and remain subject to MaxMind's license.

## Privacy

- Socket tables, process names, connection lists, and settings are handled locally.
- The server binds only to `127.0.0.1`.
- The browser UI and API are served by the CLI process.
- Online GeoIP can send hop and destination IP addresses to `ip-api.com` or `ipwho.is` after the first-run privacy notice is accepted.
- Reverse DNS may query hostnames and airport codes.
- Enable **Local geolocation** with a GeoLite2 database to avoid online GeoIP lookups.

See [SECURITY.md](./SECURITY.md) for reporting guidance.

## Limitations

- HTTPS paths and payloads remain encrypted and are not visible.
- UDP peers are not currently collected cross-platform.
- Short-lived connections may disappear between polling intervals. Connections observed before teardown keep their exact socket-to-process attribution through `TIME_WAIT`.
- Sockets whose owner cannot be proven are shown separately as **Unattributed traffic** rather than being presented as an application.
- Per-application byte rates require native OS telemetry and are reported as unavailable by the portable collector.
- Free online GeoIP services have rate limits and their own terms.

## License

[MIT](./LICENSE) © Jonathan Bangert

Earth texture and dependency attribution: [docs/ATTRIBUTIONS.md](./docs/ATTRIBUTIONS.md).

See [CHANGELOG.md](./CHANGELOG.md) for release history.
