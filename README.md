# Network Cartographer

A local network monitor that shows which applications connect to the internet, where they connect, and the routes traffic takes on an interactive 3D globe.

[CI](https://github.com/jonbng/network-cartographer/actions/workflows/ci.yml) · [MIT License](./LICENSE)

## Try it

macOS or Linux:

```bash
curl -fsSL https://mapmy.network/run | sh
```

Windows PowerShell (x64 or ARM64):

```powershell
irm https://mapmy.network/run.ps1 | iex
```

This starts Network Cartographer as a local CLI and opens the browser UI. Stop it with Ctrl+C; the process exits and cleans itself up.

## Features

- Live per-application TCP and connected UDP peer monitoring
- Native ownership collectors (`sock_diag`/procfs on Linux, libproc on macOS, and IP Helper on Windows)
- Parent-application grouping with the concrete helper PIDs retained for inspection
- Event-assisted short-lived TCP discovery when Linux permits socket-diagnostic subscriptions, with portable polling fallback
- Adaptive 250 ms–1 s socket polling that becomes more responsive while connection activity is changing
- Destination IP, domain, port, organization, and activity, with local DNS/SNI evidence when available
- Confidence-labelled destination names with stable best guesses for ambiguous shared CDN addresses
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

The CLI reads platform-native socket tables, maps TCP connections and connected UDP sockets to processes, and streams snapshots to the local UI using Server-Sent Events. Linux brackets its kernel socket dump with process-FD scans to reduce ownership races; macOS reads process descriptors through libproc; Windows reads owner-PID tables through IP Helper and inspects accessible connected UDP handles. Dashboard titles use native application metadata, including Linux desktop-entry names, macOS bundle display names, and Windows version-resource descriptions, while expanded details retain the concrete executable and PID. Missing or ambiguous metadata falls back to the executable name. It runs as the current user and does not request elevated permissions. If traceroute is unavailable, connection monitoring still works.

## Geolocation and privacy

Connection, process, DNS-cache, and SNI observation data stays in the local CLI process. Public hop and destination IPs may be sent to `https://mapmy.network/api/v1/geo` for approximate geolocation. The app also locates the public address observed by `https://mapmy.network/api/v1/egress` to mark the primary network exit. Enable **Local geolocation** with a MaxMind database to suppress hosted geolocation and keep lookups offline.

The exit marker describes where the public internet sees this connection, not the device's physical location. VPN/proxy labels use explicit system proxy and default tunnel-interface evidence; an absence of evidence does not prove a direct connection.

On Windows, destination identification watches changes in the current user's DNS client cache. Other platforms retain reverse DNS unless a local integration supplies exact TLS SNI. Integrations can discover the authenticated per-run feed in the `network-cartographer/runtime/observation-feed-<pid>.json` file below the user configuration directory, then POST `hostname`, `remoteIp`, and optional `remotePort`, `pid`, `localIp`, and `localPort` fields to its endpoint. The feed is loopback-only, uses a random bearer token, and is removed on normal shutdown.

For offline geolocation, enable **Local geolocation** and provide MaxMind databases:

| Variable                        | Database          |
| ------------------------------- | ----------------- |
| `NETWORK_CARTOGRAPHER_MMDB`     | GeoLite2-City.mmdb |
| `NETWORK_CARTOGRAPHER_ASN_MMDB` | GeoLite2-ASN.mmdb  |

The app searches the project `data/` directory and common system GeoIP locations, or accepts absolute paths through these variables. MaxMind databases are ignored by Git and remain subject to MaxMind's license. See [site/README.md](./site/README.md) and [geo-service](./geo-service/) for hosted-service details.

## Limitations

- Network Cartographer reads socket metadata, not packet contents, and does not inspect HTTPS or other application payloads
- Automatic OS DNS correlation is currently Windows-only. DNS-over-HTTPS and encrypted ClientHello can hide domain evidence, while shared CDN addresses may occasionally inherit the wrong recently observed name
- UDP coverage is limited to connected sockets. Destinations used only through unconnected `sendto()` calls require elevated packet capture and are not visible
- Collection runs as the current user, so protected or higher-integrity processes may not expose every socket or enough metadata for attribution. Unprovable owners appear as **Unattributed traffic**
- TCP discovery is polling-based on macOS and Windows. Linux can supplement polling with kernel TCP close events when permitted, but isolated short-lived connections may still be missed or recovered without their process owner
- Exceptionally large macOS socket scans are capped for safety; any truncated records and inaccessible processes are reported in the dashboard health panel
- Automatic traceroutes require repeated, attributed TCP evidence. UDP-only, one-off, and unattributed destinations remain visible but must be traced manually from the route inspector
- Per-application upload/download rates are currently available only on Linux
- Traceroutes depend on platform tools and network replies, and geolocation remains approximate; routes may be partial and the hosted geolocation service may be unavailable
- Per-application split tunneling can use exits other than the single primary exit marker

## License

[MIT](./LICENSE) © Jonathan Bangert
