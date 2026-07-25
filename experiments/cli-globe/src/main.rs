//! Network Cartographer terminal frontend.

mod app;
mod data;
mod gfx;
mod globe;
mod input;
mod mock;
mod ui;

use anyhow::{Context, Result};
use app::{Action, App, Effect, Overlay, Pane};
use crossterm::{
    event::{self, Event, KeyEventKind, MouseButton, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use data::{SourceCommand, SourceEvent, SourceHandle};
use gfx::{
    detect_cell_px, framebuffer_size, kitty_delete, kitty_place_rgb, sync_begin, sync_end,
    GfxBackend,
};
use globe::GlobeRenderer;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    env,
    io::{self, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

const KITTY_ID_A: u32 = 42;
const KITTY_ID_B: u32 = 43;

fn earth_texture_path() -> PathBuf {
    if let Ok(path) = env::var("NETWORK_CARTOGRAPHER_EARTH") {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui/public/earth-dark.jpg")
}

fn main() -> Result<()> {
    let demo = env::args().any(|arg| arg == "--demo");
    let dump_frame = env::args().any(|arg| arg == "--dump-frame");
    let texture_path = earth_texture_path();
    let texture = image::open(&texture_path)
        .with_context(|| format!("load earth texture from {}", texture_path.display()))?;
    let globe = GlobeRenderer::from_image(texture);
    let backend = GfxBackend::detect();
    let cell_px = detect_cell_px();

    if dump_frame {
        let mut app = App::new(mock::demo_snapshot(), globe, backend, cell_px);
        app.globe.frame_paths(app::default_zoom(backend));
        let frame = app.globe.render(480, 360);
        let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dump-frame.png");
        frame.save(&output)?;
        eprintln!(
            "wrote {} ({}x{})",
            output.display(),
            frame.width(),
            frame.height()
        );
        return Ok(());
    }

    let source = data::spawn_source(demo);
    let initial = match source.events.recv_timeout(Duration::from_secs(2)) {
        Ok(SourceEvent::Snapshot(snapshot)) => snapshot,
        Ok(SourceEvent::Error(error)) => {
            eprintln!("monitor unavailable: {error}");
            data::Snapshot::default()
        }
        Err(_) => data::Snapshot::default(),
    };
    let mut app = App::new(initial, globe, backend, cell_px);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        event::EnableMouseCapture,
        crossterm::cursor::Hide
    )?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    terminal.clear()?;

    let result = run_loop(&mut terminal, &mut app, &source);

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

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    source: &SourceHandle,
) -> Result<()> {
    let mut last_frame_at = Instant::now();
    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_frame_at).as_secs_f32().min(0.1);
        last_frame_at = now;
        if dt > 0.0 {
            app.fps = app.fps * 0.9 + (1.0 / dt) * 0.1;
        }

        while let Ok(event) = source.events.try_recv() {
            match event {
                SourceEvent::Snapshot(snapshot) => app.apply_snapshot(snapshot),
                SourceEvent::Error(error) => {
                    app.status = format!("Monitor error · {error}");
                    app.ui_dirty = true;
                }
            }
        }

        if app.spin && !app.dragging && app.overlay == Overlay::None {
            app.globe.yaw += dt * 0.25;
            app.globe_dirty = true;
        }
        if app.animate_flow
            && !app.globe.paths.is_empty()
            && app.globe_inner.width > 0
            && app.overlay == Overlay::None
        {
            app.globe.flow_phase = (app.globe.flow_phase + dt * 0.24).fract();
            app.globe_dirty = true;
        }

        while event::poll(Duration::ZERO)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if let Some(action) = input::key_action(app, key) {
                        let old_backend = app.backend;
                        let effect = app.update(action);
                        if old_backend == GfxBackend::Kitty && app.backend != GfxBackend::Kitty {
                            let _ = kitty_delete(KITTY_ID_A);
                            let _ = kitty_delete(KITTY_ID_B);
                        }
                        if handle_effect(effect, source)? {
                            return Ok(());
                        }
                    }
                }
                Event::Mouse(mouse) => handle_mouse(app, mouse),
                Event::Resize(_, _) => {
                    app.cell_px = detect_cell_px();
                    app.globe_inner = ratatui::layout::Rect::default();
                    app.last_frame = None;
                    app.frame_px = (0, 0);
                    app.globe_dirty = true;
                    app.ui_dirty = true;
                    terminal.clear()?;
                }
                _ => {}
            }
        }

        // Establish responsive geometry before sizing the framebuffer.
        if app.globe_inner.width == 0 && app.ui_dirty {
            terminal.draw(|frame| ui::render(frame, app))?;
        }

        let mut frame_updated = false;
        let area = app.globe_inner;
        if area.width > 1 && area.height > 1 {
            let size = framebuffer_size(app.backend, area.width, area.height, app.cell_px);
            if app.frame_px != size {
                app.frame_px = size;
                app.globe_dirty = true;
            }
            if app.globe_dirty || app.last_frame.is_none() {
                app.last_frame = Some(app.globe.render(size.0, size.1));
                app.globe_dirty = false;
                frame_updated = true;
            }
        }

        if frame_updated || app.ui_dirty {
            let mut out = io::stdout();
            let _ = sync_begin(&mut out);
            terminal.draw(|frame| ui::render(frame, app))?;

            if app.backend == GfxBackend::Kitty && frame_updated {
                place_kitty_frame(app);
            }
            let _ = sync_end(&mut out);
            let _ = out.flush();
            app.ui_dirty = false;
        }

        let wait = if app.spin || (app.animate_flow && app.globe_inner.width > 0) {
            Duration::from_millis(66)
        } else {
            Duration::from_millis(80)
        };
        let _ = event::poll(wait)?;
    }
}

fn handle_effect(effect: Effect, source: &SourceHandle) -> Result<bool> {
    match effect {
        Effect::None => Ok(false),
        Effect::Quit => Ok(true),
        Effect::ApplySettings(settings) => {
            source
                .commands
                .send(SourceCommand::ApplySettings(settings))?;
            Ok(false)
        }
        Effect::TraceAll => {
            source.commands.send(SourceCommand::TraceAll)?;
            Ok(false)
        }
        Effect::Reset => {
            source.commands.send(SourceCommand::Reset)?;
            Ok(false)
        }
    }
}

fn handle_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) {
    let (x, y) = (mouse.column, mouse.row);
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if point_in_rect(x, y, app.globe_inner) {
                app.pane = Pane::Globe;
                app.dragging = true;
                app.drag_last = Some((x, y));
                app.spin = false;
                app.ui_dirty = true;
            } else if point_in_rect(x, y, app.sidebar_inner) {
                app.pane = Pane::Applications;
                let list_y = app.sidebar_inner.y.saturating_add(1);
                if y >= list_y {
                    let index = (y - list_y) as usize;
                    if index < app.visible_rows().len() {
                        app.selected = index;
                    }
                }
                app.ui_dirty = true;
            }
        }
        MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved if app.dragging => {
            if let Some((last_x, last_y)) = app.drag_last {
                let dx = x as i32 - last_x as i32;
                let dy = y as i32 - last_y as i32;
                if dx != 0 || dy != 0 {
                    app.orbit_drag(dx, dy);
                    app.drag_last = Some((x, y));
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.dragging = false;
            app.drag_last = None;
        }
        MouseEventKind::ScrollUp if point_in_rect(x, y, app.globe_inner) => {
            let _ = app.update(Action::ZoomIn);
        }
        MouseEventKind::ScrollDown if point_in_rect(x, y, app.globe_inner) => {
            let _ = app.update(Action::ZoomOut);
        }
        _ => {}
    }
}

fn place_kitty_frame(app: &mut App) {
    let Some(frame) = &app.last_frame else { return };
    let area = app.globe_inner;
    if area.width <= 1 || area.height <= 1 {
        return;
    }
    let back = if app.kitty_front == KITTY_ID_A {
        KITTY_ID_B
    } else {
        KITTY_ID_A
    };
    let _ = io::stdout().execute(crossterm::cursor::MoveTo(area.x, area.y));
    if kitty_place_rgb(frame, area.width, area.height, back).is_ok() {
        let old = app.kitty_front;
        app.kitty_front = back;
        let _ = kitty_delete(old);
    }
}

fn point_in_rect(x: u16, y: u16, rect: ratatui::layout::Rect) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}
