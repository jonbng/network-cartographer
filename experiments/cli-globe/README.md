# Network Cartographer terminal frontend

A terminal-native frontend for Network Cartographer. It uses the same Rust
monitor, snapshot DTOs, settings, collectors, traceroute engine, and GeoIP
pipeline as the browser product—without starting the HTTP server or a browser.

The globe is software-rendered and displayed through Kitty graphics when
available, with Unicode Braille and half-block fallbacks.

## Run

```bash
cd experiments/cli-globe
cargo run --release
```

The first launch shows the same privacy choice as desktop. Select online or
local-only GeoIP before monitoring starts. Settings are shared with desktop.

For representative data without reading live sockets:

```bash
cargo run --release -- --demo
```

Render a non-interactive PNG smoke-test:

```bash
cargo run --release -- --dump-frame
```

## Navigation

| Input | Action |
|---|---|
| `Tab` / `Shift+Tab` | Switch Applications / Globe pane |
| `↑` / `↓` | Navigate applications, or orbit while Globe is active |
| `←` / `→` | Orbit while Globe is active |
| `Enter` | Expand and exclusively focus an application |
| `Space` | Add/remove an application from multi-focus |
| `/` | Filter by app, host, IP, organization, city, or hop |
| `Esc` | Close an overlay, show all, or clear the filter |
| `r` | Recenter on active paths |
| `g` | Cycle all hops / destinations / hubs |
| `l` | Toggle destination labels |
| `a` | Toggle directional data-flow pulses |
| `t` | Re-trace all destinations |
| `s` | Settings |
| `?` | Help |
| `d` | Renderer diagnostics and backend selection |
| `p` | Toggle auto-rotate |
| `+` / `-` / `0` | Zoom in / out / reset |
| mouse drag / scroll | Orbit / zoom globe |
| `q` | Quit |

At widths below 96 columns, one pane is shown at a time and `Tab` switches
between them. Wider terminals use the desktop-style applications sidebar on
the left and globe on the right.

## Graphics backends

Backend selection is automatic. Open diagnostics with `d`, then use `b`, `h`,
or `k` to switch. It can also be forced before launch:

```bash
NETWORK_CARTOGRAPHER_GFX=braille cargo run --release
NETWORK_CARTOGRAPHER_GFX=halfblocks cargo run --release
NETWORK_CARTOGRAPHER_GFX=kitty cargo run --release
```

| Backend | Density | Notes |
|---|---|---|
| Kitty | Real pixels | Best in Kitty and Ghostty |
| Braille | 2×4 dots/cell | Portable default, highest text-cell density |
| Half-blocks | 1×2 pixels/cell | Broad truecolor fallback |

The framebuffer accounts for terminal cell aspect ratio and frames are wrapped
in DEC synchronized updates. Braille and half-block modes use short native
terminal-text destination labels; Kitty composites compact pixel labels into
the high-resolution image itself. The Kitty path ping-pongs image IDs to avoid
blanking between frames.

## Architecture

```text
Shared Rust monitor + SnapshotDto
              │
              ▼
       data source thread
              │
              ▼
 app state/actions ──► responsive ratatui chrome
              │
              └──────► software globe framebuffer
                              ├─ Kitty graphics
                              ├─ Braille
                              └─ half-blocks
```

The desktop shell remains enabled by the backend crate's default `desktop`
feature. This crate disables that feature, so live monitoring is shared without
linking the HTTP server or browser-launch code.
