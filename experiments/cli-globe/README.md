# hopglobe CLI experiment (terminal-native)

**No HTML. No WebView. No Carbonyl.** This is a separate crate under `experiments/` that does **not** modify hopglobe core source.

## What this is testing

Can we rebuild hopglobe’s *feel* as a **native TUI** that uses modern terminal capabilities?

| Layer | Approach |
|-------|----------|
| Globe | Software-rendered equirectangular sphere + great-circle arcs → RGB framebuffer |
| High-res pixels | **Kitty graphics protocol** (Kitty, Ghostty, …) — real bitmaps in the terminal |
| Fallback | **Unicode halfblocks** (`▀` + truecolor fg/bg) — ~2× vertical resolution, works everywhere |
| Chrome | **ratatui** — header, app sidebar, keybindings |

This is the right direction for “the whole UI in the CLI,” not “run Chromium and paint HTML into cells.”

## Why this works in 2025–2026

Modern terminal emulators are no longer character-only:

1. **Kitty graphics protocol** — stream RGB/PNG into the terminal at **font-pixel** resolution; replace frames in place for animation.
2. **Sixel / iTerm2** — same idea, different encoding (usable via crates like `ratatui-image` later).
3. **Truecolor + Unicode mosaics** — halfblocks / braille / sextants give dense “fake pixels” without any graphics protocol.
4. **Mature TUI stacks** — `ratatui` (Rust), Charm Bubble Tea, Ink, Notcurses — production TUIs with mouse, layout, and (via helpers) images.

For a live globe, the winning architecture is:

```
Rust backend (reuse hopglobe collect/trace/geo later)
        │
        ▼
Software or GPU offscreen globe → RGBA frame
        │
        ├─► Kitty/Sixel/iTerm2  (sharp, high DPI)
        └─► halfblocks/braille  (portable fallback)
        │
        ▼
ratatui chrome (lists, filters, status)
```

The existing Tauri + globe.gl UI stays the desktop product; this experiment is a **second front-end** on the same data.

## Run

From the repo root (or this directory):

```bash
cd experiments/cli-globe
cargo run --release
```

Optional:

```bash
# force backend
HOPGLOBE_GFX=braille    cargo run --release   # default portable
HOPGLOBE_GFX=halfblocks cargo run --release
HOPGLOBE_GFX=kitty      cargo run --release   # needs Kitty/Ghostty/etc.

# non-interactive smoke: write a PNG of one frame
cargo run --release -- --dump-frame
```

### Keys & mouse

| Input | Action |
|-----|--------|
| **click-drag on globe** | Orbit (yaw / pitch) |
| scroll on globe | Zoom |
| `←` `→` `↑` `↓` | Orbit |
| `space` | Toggle spin (off by default) |
| `+` / `-` / `0` | Zoom in / out / reset |
| `tab` | Focus next app |
| `b` / `h` / `k` | Braille / halfblocks / kitty |
| `q` | Quit |

### Graphics backends

| Backend | Density | Notes |
|---------|---------|--------|
| **braille** (default when no Kitty) | 2×4 dots/cell | ~4× halfblocks; best portable quality |
| **halfblocks** | 1×2 px/cell | Classic `▀` truecolor |
| **kitty** | real pixels | Kitty / Ghostty; needs correct cell size |

```bash
HOPGLOBE_GFX=braille cargo run --release
HOPGLOBE_GFX=halfblocks cargo run --release
HOPGLOBE_GFX=kitty cargo run --release
```

### Rendering notes

- Framebuffer is sized to the **panel’s display aspect** (accounts for tall character cells) so the sphere stays round, not stretched.
- Frames are wrapped in **DEC synchronized update** (`CSI ?2026`) to reduce flicker.
- Kitty path **ping-pongs** two image ids and marks the globe cells as `skip` so ratatui doesn’t wipe the bitmap mid-frame.
- Prefer **`b` (braille)** if Kitty flickers or stretches — denser mosaic, no graphics protocol.

### Texture

Uses `ui/public/earth-dark.jpg` from the main app (or set `HOPGLOBE_EARTH=/path/to.jpg`).

## Recommended terminals

| Terminal | Best path |
|----------|-----------|
| **Kitty** | Kitty protocol (pixel) |
| **Ghostty** | Kitty protocol |
| **WezTerm** | iTerm2/Kitty (try `HOPGLOBE_GFX=kitty` or halfblocks) |
| **foot** | Sixel (wire via `ratatui-image` later) or halfblocks |
| **Pretty much anything else** | Halfblocks (still looks good) |

## Next steps (if the experiment sticks)

1. Wire **real** monitor snapshots (IPC to the Tauri/Rust core, or link `hopglobe` crates as a library).
2. Swap the soft sphere for a faster path (SIMD, or offscreen `wgpu` → same framebuffer).
3. Use `ratatui-image` + `ThreadProtocol` for automatic protocol detection and non-blocking encode.
4. Braille mode for denser fallback (2×4 dots/cell).
5. Keep core `src-tauri` / `ui` untouched — CLI stays a sibling frontend.

## Non-goals of this folder

- Not a replacement for the desktop app yet
- Not shipping as the main binary
- Not modifying `ui/` or `src-tauri/`
