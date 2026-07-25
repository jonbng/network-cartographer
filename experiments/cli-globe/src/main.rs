//! hopglobe CLI experiment — terminal-native UI (no HTML, no WebView).
//!
//! Does not touch hopglobe core source. Mock paths only for now.

mod gfx;
mod globe;
mod mock;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
    ExecutableCommand,
};
use gfx::{
    detect_cell_px, framebuffer_size, kitty_delete, kitty_place_rgb, mark_skip, sync_begin,
    sync_end, BrailleImage, CellPx, GfxBackend, HalfblockImage,
};
use globe::GlobeRenderer;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use std::{
    env, io,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

/// Ping-pong Kitty image ids so we never blank a frame with delete-before-draw.
const KITTY_ID_A: u32 = 42;
const KITTY_ID_B: u32 = 43;

struct App {
    globe: GlobeRenderer,
    backend: GfxBackend,
    spin: bool,
    focus: usize,
    status: String,
    last_frame: Option<image::RgbImage>,
    /// Inner globe panel (inside borders) — hit-test + blit target.
    globe_inner: Rect,
    fps: f32,
    /// Left-drag orbit state (cell coordinates).
    dragging: bool,
    drag_last: Option<(u16, u16)>,
    kitty_front: u32,
    needs_full_chrome: bool,
    frame_px: (u32, u32),
    /// Rebuild globe buffer when camera/layout changes (avoids idle spin of CPU).
    globe_dirty: bool,
    /// Terminal font cell size in pixels (for correct Kitty aspect).
    cell_px: CellPx,
}

fn find_earth_texture() -> PathBuf {
    if let Ok(p) = env::var("HOPGLOBE_EARTH") {
        return PathBuf::from(p);
    }
    let candidates = [
        "../../ui/public/earth-dark.jpg",
        "../ui/public/earth-dark.jpg",
        "ui/public/earth-dark.jpg",
        "../../../ui/public/earth-dark.jpg",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("/home/jonathan/code/hopglobe/ui/public/earth-dark.jpg")
}

fn main() -> Result<()> {
    let tex_path = find_earth_texture();
    let img = image::open(&tex_path)
        .with_context(|| format!("load earth texture from {}", tex_path.display()))?;

    let mut globe = GlobeRenderer::from_image(img);
    globe.paths = mock::demo_paths();

    let backend = GfxBackend::detect();
    let cell_px = detect_cell_px();
    let z0 = default_zoom(backend);
    globe.set_zoom(z0);

    let mut app = App {
        globe,
        backend,
        // Manual camera by default — no auto-spin.
        spin: false,
        focus: 0,
        status: format!(
            "gfx={} · cell {}×{}px · drag orbit · scroll zoom · b/h/k gfx · q quit  (zoom {:.1}×)",
            backend.label(),
            cell_px.w,
            cell_px.h,
            z0
        ),
        last_frame: None,
        globe_inner: Rect::default(),
        fps: 0.0,
        dragging: false,
        drag_last: None,
        kitty_front: KITTY_ID_A,
        needs_full_chrome: true,
        frame_px: (0, 0),
        globe_dirty: true,
        cell_px,
    };

    if env::args().any(|a| a == "--dump-frame") {
        // Square-ish dump for smoke tests
        let frame = app.globe.render(240, 240);
        let out = if PathBuf::from("Cargo.toml").exists() {
            PathBuf::from("dump-frame.png")
        } else {
            PathBuf::from("experiments/cli-globe/dump-frame.png")
        };
        frame.save(&out)?;
        eprintln!(
            "wrote {} ({}x{})",
            out.display(),
            frame.width(),
            frame.height()
        );
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        event::EnableMouseCapture,
        crossterm::cursor::Hide
    )?;
    let backend_term = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend_term)?;
    // Don't clear every draw more than needed — ratatui already diffs.
    terminal.clear()?;

    let tick = Duration::from_millis(33);
    let mut last = Instant::now();
    let result = run_loop(&mut terminal, &mut app, tick, &mut last);

    if app.backend == GfxBackend::Kitty {
        let _ = kitty_delete(KITTY_ID_A);
        let _ = kitty_delete(KITTY_ID_B);
    }
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        event::DisableMouseCapture,
        crossterm::cursor::Show
    )?;
    result
}

fn point_in_rect(x: u16, y: u16, r: Rect) -> bool {
    x >= r.x && x < r.x.saturating_add(r.width) && y >= r.y && y < r.y.saturating_add(r.height)
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    tick: Duration,
    last: &mut Instant,
) -> Result<()> {
    loop {
        let now = Instant::now();
        let dt = now.duration_since(*last).as_secs_f32().min(0.1);
        *last = now;
        if dt > 0.0 {
            app.fps = 0.9 * app.fps + 0.1 * (1.0 / dt);
        }

        if app.spin && !app.dragging {
            app.globe.yaw += dt * 0.35;
            app.globe_dirty = true;
        }

        // Drain all pending input so drag stays smooth.
        while event::poll(Duration::from_millis(0))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => match k.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char(' ') => {
                        app.spin = !app.spin;
                        app.status = if app.spin {
                            "spin on".into()
                        } else {
                            "spin off · drag or arrows to orbit".into()
                        };
                    }
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        app.globe.zoom_by(1.12);
                        app.status = format!("zoom {:.1}×", app.globe.zoom);
                        app.globe_dirty = true;
                    }
                    KeyCode::Char('-') | KeyCode::Char('_') => {
                        app.globe.zoom_by(1.0 / 1.12);
                        app.status = format!("zoom {:.1}×", app.globe.zoom);
                        app.globe_dirty = true;
                    }
                    KeyCode::Char('0') => {
                        let z = default_zoom(app.backend);
                        app.globe.set_zoom(z);
                        app.status = format!("zoom reset {:.1}×", app.globe.zoom);
                        app.globe_dirty = true;
                    }
                    KeyCode::Left => {
                        app.spin = false;
                        app.globe.yaw -= 0.12;
                        app.globe_dirty = true;
                    }
                    KeyCode::Right => {
                        app.spin = false;
                        app.globe.yaw += 0.12;
                        app.globe_dirty = true;
                    }
                    KeyCode::Up => {
                        app.spin = false;
                        app.globe.pitch = (app.globe.pitch + 0.08).clamp(-1.2, 1.2);
                        app.globe_dirty = true;
                    }
                    KeyCode::Down => {
                        app.spin = false;
                        app.globe.pitch = (app.globe.pitch - 0.08).clamp(-1.2, 1.2);
                        app.globe_dirty = true;
                    }
                    KeyCode::Tab => {
                        let n = app.globe.paths.len().max(1);
                        app.focus = (app.focus + 1) % n;
                        app.needs_full_chrome = true;
                    }
                    KeyCode::BackTab => {
                        let n = app.globe.paths.len().max(1);
                        app.focus = (app.focus + n - 1) % n;
                        app.needs_full_chrome = true;
                    }
                    KeyCode::Char('b') => {
                        switch_backend(app, GfxBackend::Braille);
                    }
                    KeyCode::Char('h') => {
                        switch_backend(app, GfxBackend::Halfblocks);
                    }
                    KeyCode::Char('k') => {
                        switch_backend(app, GfxBackend::Kitty);
                    }
                    _ => {}
                },
                Event::Mouse(m) => handle_mouse(app, m),
                Event::Resize(_, _) => {
                    app.cell_px = detect_cell_px();
                    app.needs_full_chrome = true;
                    app.last_frame = None;
                    app.globe_dirty = true;
                    terminal.clear()?;
                }
                _ => {}
            }
        }

        // Render globe framebuffer to match panel aspect (no stretch).
        let mut frame_updated = false;
        let area = app.globe_inner;
        if area.width > 1 && area.height > 1 {
            let (pw, ph) =
                framebuffer_size(app.backend, area.width, area.height, app.cell_px);
            if app.frame_px != (pw, ph) {
                app.frame_px = (pw, ph);
                app.globe_dirty = true;
            }
            if app.globe_dirty || app.last_frame.is_none() {
                app.last_frame = Some(app.globe.render(pw, ph));
                app.globe_dirty = false;
                frame_updated = true;
            }
        }

        // Atomic frame: synchronized update wraps ratatui draw + kitty blit.
        // Only redraw when camera/layout changed (static globe stays put — no flicker).
        let redraw = frame_updated || app.needs_full_chrome;
        if redraw {
            {
                let mut out = io::stdout();
                let _ = sync_begin(&mut out);
            }

            terminal.draw(|f| ui(f, app))?;

            if app.backend == GfxBackend::Kitty && frame_updated {
                if let Some(frame) = &app.last_frame {
                    let a = app.globe_inner;
                    if a.width > 1 && a.height > 1 {
                        let cols = a.width;
                        let rows = a.height;
                        let back = if app.kitty_front == KITTY_ID_A {
                            KITTY_ID_B
                        } else {
                            KITTY_ID_A
                        };
                        let _ = io::stdout().execute(crossterm::cursor::MoveTo(a.x, a.y));
                        if kitty_place_rgb(frame, cols, rows, back).is_ok() {
                            let old = app.kitty_front;
                            app.kitty_front = back;
                            let _ = kitty_delete(old);
                        }
                    }
                }
            }

            {
                let mut out = io::stdout();
                let _ = sync_end(&mut out);
                let _ = out.flush();
            }
        }

        app.needs_full_chrome = false;

        // Idle: wait for input instead of burning CPU redrawing a static globe.
        let wait = if app.spin {
            tick.saturating_sub(now.elapsed())
        } else {
            Duration::from_millis(50)
        };
        let _ = event::poll(wait)?;
    }
}

fn default_zoom(backend: GfxBackend) -> f32 {
    GlobeRenderer::default_zoom_for_backend(
        backend == GfxBackend::Kitty,
        backend == GfxBackend::Braille,
    )
}

fn switch_backend(app: &mut App, backend: GfxBackend) {
    if app.backend == GfxBackend::Kitty && backend != GfxBackend::Kitty {
        let _ = kitty_delete(KITTY_ID_A);
        let _ = kitty_delete(KITTY_ID_B);
    }
    app.backend = backend;
    if backend == GfxBackend::Kitty {
        app.cell_px = detect_cell_px();
    }
    app.globe.set_zoom(default_zoom(backend));
    app.status = match backend {
        GfxBackend::Braille => format!(
            "braille (2×4 dots/cell) · zoom {:.1}× · b/h/k switch",
            app.globe.zoom
        ),
        GfxBackend::Halfblocks => format!("halfblocks · zoom {:.1}× · b/h/k switch", app.globe.zoom),
        GfxBackend::Kitty => format!(
            "kitty · cell {}×{}px · zoom {:.1}× · b/h/k switch",
            app.cell_px.w, app.cell_px.h, app.globe.zoom
        ),
    };
    app.needs_full_chrome = true;
    app.last_frame = None;
    app.globe_dirty = true;
}

/// Grab-the-globe orbit: drag right moves the surface with your cursor
/// (yaw decreases), drag down tips the north pole toward you.
fn apply_orbit_drag(app: &mut App, dx: i32, dy: i32) {
    let sens = 0.045 / app.globe.zoom.sqrt();
    app.globe.yaw -= dx as f32 * sens;
    app.globe.pitch = (app.globe.pitch + dy as f32 * sens * 0.8).clamp(-1.2, 1.2);
    app.globe_dirty = true;
}

fn handle_mouse(app: &mut App, m: crossterm::event::MouseEvent) {
    let (x, y) = (m.column, m.row);
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if point_in_rect(x, y, app.globe_inner) {
                app.dragging = true;
                app.drag_last = Some((x, y));
                app.spin = false;
                app.status = "dragging · release to stop".into();
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            // Some terminals only send Drag; others send Moved while held.
            if !app.dragging && point_in_rect(x, y, app.globe_inner) {
                app.dragging = true;
                app.drag_last = Some((x, y));
                app.spin = false;
            }
            if app.dragging {
                if let Some((lx, ly)) = app.drag_last {
                    let dx = x as i32 - lx as i32;
                    let dy = y as i32 - ly as i32;
                    apply_orbit_drag(app, dx, dy);
                    app.drag_last = Some((x, y));
                } else {
                    app.drag_last = Some((x, y));
                }
            }
        }
        MouseEventKind::Moved => {
            // Fallback: if terminal doesn't emit Drag, track while already dragging.
            if app.dragging {
                if let Some((lx, ly)) = app.drag_last {
                    let dx = x as i32 - lx as i32;
                    let dy = y as i32 - ly as i32;
                    if dx != 0 || dy != 0 {
                        apply_orbit_drag(app, dx, dy);
                        app.drag_last = Some((x, y));
                    }
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if app.dragging {
                app.dragging = false;
                app.drag_last = None;
                app.status = "drag ended · space to spin · q quit".into();
            }
        }
        MouseEventKind::ScrollUp => {
            if point_in_rect(x, y, app.globe_inner) || app.globe_inner.width == 0 {
                app.globe.zoom_by(1.10);
                app.status = format!("zoom {:.1}× · drag to orbit", app.globe.zoom);
                app.globe_dirty = true;
            }
        }
        MouseEventKind::ScrollDown => {
            if point_in_rect(x, y, app.globe_inner) || app.globe_inner.width == 0 {
                app.globe.zoom_by(1.0 / 1.10);
                app.status = format!("zoom {:.1}× · drag to orbit", app.globe.zoom);
                app.globe_dirty = true;
            }
        }
        _ => {}
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(f.area());

    let n_apps = app.globe.paths.len();
    let n_hops: usize = app.globe.paths.iter().map(|p| p.hops.len()).sum();
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " HG ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" hopglobe "),
        Span::styled("cli", Style::default().fg(Color::DarkGray)),
        Span::raw("  ·  "),
        Span::styled(format!("{n_apps} apps"), Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled(format!("{n_hops} hops"), Style::default().fg(Color::Magenta)),
        Span::raw("  "),
        Span::styled(
            format!("{:.0} fps", app.fps),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  "),
        Span::styled(app.backend.label(), Style::default().fg(Color::Yellow)),
        Span::raw("  "),
        Span::styled(
            format!("{}×{}", app.frame_px.0, app.frame_px.1),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{:.1}×", app.globe.zoom),
            Style::default().fg(Color::Green),
        ),
        Span::raw("  "),
        Span::styled(
            format!("cell {}×{}", app.cell_px.w, app.cell_px.h),
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(Block::default().borders(Borders::ALL).title("hopglobe"));
    f.render_widget(header, root[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(root[1]);

    let globe_block = Block::default()
        .borders(Borders::ALL)
        .title("globe · drag orbit · scroll zoom");
    let globe_inner = globe_block.inner(body[0]);
    f.render_widget(globe_block, body[0]);
    app.globe_inner = globe_inner;

    match app.backend {
        GfxBackend::Braille => {
            if let Some(frame) = &app.last_frame {
                f.render_widget(BrailleImage { img: frame }, globe_inner);
            }
        }
        GfxBackend::Halfblocks => {
            if let Some(frame) = &app.last_frame {
                f.render_widget(HalfblockImage { img: frame }, globe_inner);
            }
        }
        GfxBackend::Kitty => {
            // Don't paint text over the image region; skip so diffs won't blank it.
            mark_skip(globe_inner, f.buffer_mut());
        }
    }

    let items: Vec<ListItem> = app
        .globe
        .paths
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let c = Color::Rgb(p.color[0], p.color[1], p.color[2]);
            let marker = if i == app.focus { "▸ " } else { "  " };
            let line = Line::from(vec![
                Span::styled(marker, Style::default().fg(c)),
                Span::styled(
                    format!("{:<10}", p.app),
                    Style::default().fg(c).add_modifier(if i == app.focus {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ),
                Span::styled(format!(" → {}", p.host), Style::default().fg(Color::Gray)),
            ]);
            let hops = p
                .hops
                .iter()
                .map(|h| h.label.clone())
                .collect::<Vec<_>>()
                .join(" · ");
            ListItem::new(vec![
                line,
                Line::from(Span::styled(
                    format!("    {hops}"),
                    Style::default().fg(Color::DarkGray),
                )),
            ])
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("applications · tab"),
    );
    f.render_widget(list, body[1]);

    let footer = Paragraph::new(app.status.as_str()).style(Style::default().fg(Color::Gray));
    f.render_widget(footer, root[2]);
}
