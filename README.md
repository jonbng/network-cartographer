# Network Cartographer

A local network monitor that shows which applications connect to the internet, where they connect, and the routes traffic takes—on an interactive 3D globe.

[CI](https://github.com/jonbng/network-cartographer/actions/workflows/ci.yml) · [MIT License](./LICENSE)

## Try it

macOS or Linux:

```bash
curl -fsSL https://mapmy.network/run | sh
```

Windows PowerShell:

```powershell
irm https://mapmy.network/run.ps1 | iex
```

This starts Network Cartographer as a local CLI and opens the browser UI. Stop it with Ctrl+C; the process exits and cleans itself up.

## Features

- Live per-application TCP connection monitoring
- Destination IP, hostname, port, organization, and activity
- Background traceroutes and hop geolocation
- Optional per-application upload/download rates on Linux
- Search, application focus, route history, and density controls
- Local browser UI served only on `127.0.0.1`

## Run from source

Requires Rust 1.88+, Node.js 18+, and a modern browser. Traceroutes use the platform's `traceroute`, `tracepath`, or `tracert` command when available.

```bash
git clone https://github.com/jonbng/network-cartographer.git
cd network-cartographer
npm install
npm start
```

## Command options

```text
netcart [--port PORT] [--no-open]

--port PORT  Local port (default: 4769)
--no-open    Do not open the browser automatically
```

Set `NETCART_DEBUG=1` for geolocation diagnostics.

## Development

Run the full application:

```bash
npm start
```

For frontend hot reload, run these in separate terminals:

```bash
cargo run --manifest-path server/Cargo.toml -- --no-open
npm run frontend:dev
```

Checks and release build:

```bash
npm run check
npm test
npm run build
cargo build --release --locked --manifest-path server/Cargo.toml
```

## Architecture

```text
ui/           Vite + TypeScript + globe.gl UI
server/       Rust CLI, monitoring, traceroutes, API, and embedded UI
geo-service/  Hosted GeoIP lookup service
site/         mapmy.network website and API proxy
experiments/  Unsupported interface experiments
```

The CLI reads OS socket tables, maps connections to processes, and streams snapshots to the local UI using Server-Sent Events. It runs as the current user and does not request elevated permissions. If traceroute is unavailable, connection monitoring still works.

## Geolocation and privacy

Connection and process data stays in the local CLI process. After the first-run privacy notice, public hop and destination IPs may be sent to `https://mapmy.network/api/v1/geo` for approximate geolocation.

For offline geolocation, enable **Local geolocation** and provide MaxMind databases:

| Variable                        | Database          |
| ------------------------------- | ----------------- |
| `NETWORK_CARTOGRAPHER_MMDB`     | GeoLite2-City.mmdb |
| `NETWORK_CARTOGRAPHER_ASN_MMDB` | GeoLite2-ASN.mmdb  |

The app searches the project `data/` directory and common system GeoIP locations, or accepts absolute paths through these variables. MaxMind databases are ignored by Git and remain subject to MaxMind's license. See [site/README.md](./site/README.md) and [geo-service](./geo-service/) for hosted-service details.

## Limitations

- TCP only; HTTPS payloads remain encrypted
- UDP peers are not collected cross-platform
- Short-lived connections may disappear between polling intervals
- Unprovable socket owners appear as **Unattributed traffic**
- Per-application byte rates depend on OS support
- Geolocation is approximate and the hosted service may be unavailable

## License

[MIT](./LICENSE) © Jonathan Bangert
