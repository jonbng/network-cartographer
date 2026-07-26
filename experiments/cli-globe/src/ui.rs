//! Responsive terminal presentation.

use crate::{
    app::{App, Overlay, Pane, RowRef},
    data::color_for_key,
    gfx::{mark_skip, BrailleImage, GfxBackend, HalfblockImage},
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap},
    Frame,
};

const BG: Color = Color::Rgb(5, 8, 13);
const PANEL: Color = Color::Rgb(10, 16, 26);
const BORDER: Color = Color::Rgb(48, 64, 86);
const TEXT: Color = Color::Rgb(232, 238, 247);
const MUTED: Color = Color::Rgb(166, 180, 198);
const ACCENT: Color = Color::Rgb(34, 211, 238);
const OK: Color = Color::Rgb(52, 211, 153);
const WARN: Color = Color::Rgb(251, 191, 36);
const BAD: Color = Color::Rgb(248, 113, 113);
const DEST: Color = Color::Rgb(249, 168, 212);

pub fn render(f: &mut Frame, app: &mut App) {
    f.render_widget(
        Block::default().style(Style::default().bg(BG).fg(TEXT)),
        f.area(),
    );
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_header(f, app, root[0]);

    if f.area().width >= 96 {
        let sidebar_width = (f.area().width / 3).clamp(30, 38);
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(sidebar_width), Constraint::Min(48)])
            .split(root[1]);
        render_applications(f, app, body[0], app.pane == Pane::Applications);
        render_globe(f, app, body[1], app.pane == Pane::Globe);
    } else if app.pane == Pane::Applications {
        render_applications(f, app, root[1], true);
        app.globe_inner = Rect::default();
    } else {
        render_globe(f, app, root[1], true);
        app.sidebar_inner = Rect::default();
    }

    render_footer(f, app, root[2]);
    render_overlay(f, app);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let line = if app.filtering {
        Line::from(vec![
            Span::styled(" / ", Style::default().fg(BG).bg(ACCENT)),
            Span::styled(&app.filter, Style::default().fg(TEXT)),
            Span::styled("█", Style::default().fg(ACCENT)),
            Span::styled("  Enter apply · Ctrl+U clear", Style::default().fg(MUTED)),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " NC ",
                Style::default()
                    .fg(Color::Rgb(4, 16, 22))
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Network Cartographer ", Style::default().fg(TEXT)),
            if app.snapshot.demo {
                Span::styled("● DEMO", Style::default().fg(WARN))
            } else if app.snapshot.settings.privacy_accepted {
                Span::styled("● LIVE", Style::default().fg(OK))
            } else {
                Span::styled("● PAUSED", Style::default().fg(WARN))
            },
            Span::styled(
                if area.width < 100 {
                    format!(
                        "  {}a  {}p  {}h  {}c",
                        app.snapshot.app_count,
                        app.snapshot.mapped_path_count(),
                        app.snapshot.mapped_hop_count(),
                        app.snapshot.live_connections
                    )
                } else {
                    format!(
                        "  {} apps  {} paths  {} hops  {} conns  ",
                        app.snapshot.app_count,
                        app.snapshot.mapped_path_count(),
                        app.snapshot.mapped_hop_count(),
                        app.snapshot.live_connections
                    )
                },
                Style::default().fg(MUTED),
            ),
            Span::styled(
                if area.width < 100 {
                    String::new()
                } else {
                    format!(
                        "tr q{} r{} ✓{}{}",
                        app.snapshot.trace_stats.queued,
                        app.snapshot.trace_stats.running,
                        app.snapshot.trace_stats.done,
                        if app.snapshot.trace_stats.failed > 0 {
                            format!(" !{}", app.snapshot.trace_stats.failed)
                        } else {
                            String::new()
                        }
                    )
                },
                Style::default().fg(if app.snapshot.trace_stats.failed > 0 {
                    BAD
                } else if app.snapshot.trace_stats.running > 0 {
                    WARN
                } else {
                    ACCENT
                }),
            ),
        ])
    };
    f.render_widget(Paragraph::new(line).style(Style::default().bg(PANEL)), area);
}

fn render_applications(f: &mut Frame, app: &mut App, area: Rect, active: bool) {
    let title_style = if active { ACCENT } else { MUTED };
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(if active { ACCENT } else { BORDER }))
        .padding(Padding::new(1, 1, 0, 0));
    let inner = block.inner(area);
    f.render_widget(block.style(Style::default().bg(PANEL)), area);
    app.sidebar_inner = inner;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);
    let sub = if app.focused.is_empty() {
        "Enter focus · Space multi-select"
    } else {
        "Esc show all · Space multi-select"
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("APPLICATIONS", Style::default().fg(title_style)),
            Span::styled(format!("  {sub}"), Style::default().fg(MUTED)),
        ])),
        chunks[0],
    );

    let rows = app.visible_rows();
    if rows.is_empty() {
        f.render_widget(
            Paragraph::new(if app.filter.is_empty() {
                "No applications with internet connections yet."
            } else {
                "No applications match this filter."
            })
            .style(Style::default().fg(MUTED))
            .wrap(Wrap { trim: true }),
            chunks[1],
        );
        return;
    }

    let items: Vec<ListItem> = rows.iter().filter_map(|row| row_item(app, row)).collect();
    let mut state = ListState::default().with_selected(Some(app.selected));
    let list = List::new(items)
        .style(Style::default().bg(PANEL).fg(TEXT))
        .highlight_style(Style::default().bg(Color::Rgb(13, 42, 52)))
        .highlight_symbol("▎ ");
    f.render_stateful_widget(list, chunks[1], &mut state);
}

fn row_item<'a>(app: &'a App, row: &RowRef) -> Option<ListItem<'a>> {
    match row {
        RowRef::App(id) => {
            let application = app
                .snapshot
                .apps
                .iter()
                .find(|application| &application.id == id)?;
            let color = rgb(color_for_key(&application.name));
            let mapped = application
                .destinations
                .iter()
                .filter(|destination| destination.hops.iter().any(|hop| hop.lat.is_some()))
                .count();
            let hops: usize = application
                .destinations
                .iter()
                .map(|destination| {
                    destination
                        .hops
                        .iter()
                        .filter(|hop| hop.lat.is_some())
                        .count()
                })
                .sum();
            let open = app.expanded.contains(id) || app.focused.contains(id);
            let dimmed = !app.focused.is_empty() && !app.focused.contains(id);
            let style = Style::default().fg(if dimmed { MUTED } else { TEXT });
            Some(ListItem::new(Line::from(vec![
                Span::styled(
                    "● ",
                    Style::default().fg(if dimmed { MUTED } else { color }),
                ),
                Span::styled(&application.name, style),
                Span::styled(
                    format!(
                        "  {mapped}/{} · {hops}h{}  {}",
                        application.destinations.len(),
                        if application.activity > 0.05 {
                            format!(" · {:.1}/s", application.activity)
                        } else {
                            String::new()
                        },
                        if open { "▾" } else { "▸" }
                    ),
                    Style::default().fg(MUTED),
                ),
            ])))
        }
        RowRef::Destination {
            app_id,
            destination_id,
        } => {
            let application = app
                .snapshot
                .apps
                .iter()
                .find(|application| &application.id == app_id)?;
            let destination = application
                .destinations
                .iter()
                .find(|destination| &destination.id == destination_id)?;
            let mapped = destination
                .hops
                .iter()
                .filter(|hop| hop.lat.is_some())
                .count();
            let right = if mapped > 0 {
                format!("{mapped} hops")
            } else {
                destination.status.clone()
            };
            let rtt = destination
                .rtt_ms
                .map(|value| format!("{value:.0}ms"))
                .unwrap_or_else(|| "-".into());
            Some(ListItem::new(Line::from(vec![
                Span::styled("  ★ ", Style::default().fg(DEST)),
                Span::styled(&destination.host, Style::default().fg(TEXT)),
                Span::styled(format!("  {rtt} · {right}"), Style::default().fg(MUTED)),
                if destination.path_changed {
                    Span::styled("  Δ", Style::default().fg(WARN))
                } else {
                    Span::raw("")
                },
            ])))
        }
    }
}

fn render_globe(f: &mut Frame, app: &mut App, area: Rect, active: bool) {
    let show_detail = area.height >= 14;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if show_detail {
            [
                Constraint::Length(1),
                Constraint::Min(7),
                Constraint::Length(3),
            ]
        } else {
            [
                Constraint::Length(1),
                Constraint::Min(7),
                Constraint::Length(0),
            ]
        })
        .split(area);

    let mode = if app.focused.is_empty() {
        "GLOBE · all routes"
    } else {
        "GLOBE · focused routes"
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {mode}"),
                Style::default().fg(if active { ACCENT } else { MUTED }),
            ),
            Span::styled(
                format!(
                    "  {} · {} · geo {} · drag/scroll",
                    app.density.label(),
                    if app.animate_flow {
                        "⇢ flow"
                    } else {
                        "flow off"
                    },
                    app.snapshot.geo_backend
                ),
                Style::default().fg(MUTED),
            ),
        ])),
        chunks[0],
    );

    app.globe_inner = chunks[1].inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    if app.globe_inner.width > 0 && app.globe_inner.height > 0 {
        match app.backend {
            GfxBackend::Braille => {
                if let Some(frame) = &app.last_frame {
                    f.render_widget(BrailleImage { img: frame }, app.globe_inner);
                }
            }
            GfxBackend::Halfblocks => {
                if let Some(frame) = &app.last_frame {
                    f.render_widget(HalfblockImage { img: frame }, app.globe_inner);
                }
            }
            GfxBackend::Kitty => mark_skip(app.globe_inner, f.buffer_mut()),
        }
        render_globe_labels(f, app);
    }

    if show_detail {
        render_selection_detail(f, app, chunks[2]);
    }
}

fn render_globe_labels(f: &mut Frame, app: &App) {
    let area = app.globe_inner;
    let (frame_width, frame_height) = app.frame_px;
    if app.backend == GfxBackend::Kitty
        || !app.globe.show_labels
        || frame_width == 0
        || frame_height == 0
        || area.width == 0
        || area.height == 0
    {
        return;
    }

    let mut occupied: Vec<Rect> = Vec::new();
    for anchor in app.globe.label_anchors(frame_width, frame_height) {
        let width = anchor.text.chars().count() as u16;
        if width == 0 || width >= area.width {
            continue;
        }
        let local_x = ((anchor.x / frame_width as f32) * area.width as f32).round() as i32;
        let local_y = ((anchor.y / frame_height as f32) * area.height as f32).round() as i32;
        let anchor_x = area.x as i32 + local_x;
        let anchor_y = area.y as i32 + local_y;

        let right_x = anchor_x + 1;
        let preferred_x = if right_x + width as i32 <= area.right() as i32 {
            right_x
        } else {
            anchor_x - width as i32 - 1
        };
        let Some(label_area) = [0, -1, 1, -2, 2].into_iter().find_map(|offset_y| {
            let x = preferred_x;
            let y = anchor_y + offset_y;
            if x < area.x as i32
                || y < area.y as i32
                || x + width as i32 > area.right() as i32
                || y >= area.bottom() as i32
            {
                return None;
            }
            let candidate = Rect::new(x as u16, y as u16, width, 1);
            occupied
                .iter()
                .all(|other| !rects_intersect(candidate, *other))
                .then_some(candidate)
        }) else {
            continue;
        };
        occupied.push(label_area);

        // Kitty cells were marked as skipped to protect the image. Labels are
        // intentional terminal text, so allow ratatui to paint these cells.
        for x in label_area.x..label_area.right() {
            if let Some(cell) = f.buffer_mut().cell_mut((x, label_area.y)) {
                cell.set_skip(false);
            }
        }
        f.render_widget(
            Paragraph::new(anchor.text).style(Style::default().fg(DEST).bg(Color::Rgb(3, 7, 13))),
            label_area,
        );
    }
}

fn rects_intersect(a: Rect, b: Rect) -> bool {
    a.x < b.right() && a.right() > b.x && a.y < b.bottom() && a.bottom() > b.y
}

fn render_selection_detail(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(BORDER))
        .padding(Padding::horizontal(1));
    let lines = if let Some((application, destination)) = app.selected_destination() {
        let final_ttl = destination.hops.last().map(|hop| hop.ttl);
        vec![
            Line::from(vec![
                Span::styled("★ ", Style::default().fg(DEST)),
                Span::styled(
                    &destination.host,
                    Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ·  {}", application.name),
                    Style::default().fg(rgb(color_for_key(&application.name))),
                ),
            ]),
            Line::from(Span::styled(
                format!(
                    "{}:{} · {} · {} · {} · {} hits{}",
                    destination.ip,
                    destination.port,
                    destination.protocol,
                    destination
                        .rtt_ms
                        .map(|rtt| format!("{rtt:.0}ms"))
                        .unwrap_or_else(|| "-".into()),
                    destination.status,
                    destination.hits,
                    final_ttl
                        .map(|ttl| format!(" · hop {ttl}"))
                        .unwrap_or_default()
                ),
                Style::default().fg(MUTED),
            )),
        ]
    } else if let Some(application) = app.selected_app() {
        vec![
            Line::from(Span::styled(
                &application.name,
                Style::default()
                    .fg(rgb(color_for_key(&application.name)))
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!(
                    "{} destinations · {:.1}/s activity",
                    application.destinations.len(),
                    application.activity
                ),
                Style::default().fg(MUTED),
            )),
        ]
    } else {
        vec![Line::from(Span::styled(
            "Select an application to inspect its routes",
            Style::default().fg(MUTED),
        ))]
    };
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(area);
    let missing = if app.snapshot.missing_pid > 0 {
        format!(" · {} without pid", app.snapshot.missing_pid)
    } else {
        String::new()
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(
                    " {} · {} destinations",
                    app.status, app.snapshot.destination_count
                ),
                Style::default().fg(MUTED),
            ),
            Span::styled(missing, Style::default().fg(WARN)),
        ]))
        .style(Style::default().bg(PANEL)),
        chunks[0],
    );
    f.render_widget(
        Paragraph::new(if area.width < 100 {
            "/ filter · Tab pane · ? help "
        } else {
            "/ filter · Enter focus · r recenter · ? help "
        })
        .alignment(Alignment::Right)
        .style(Style::default().fg(MUTED).bg(PANEL)),
        chunks[1],
    );
}

fn render_overlay(f: &mut Frame, app: &App) {
    let (title, lines, width, height) = match app.overlay {
        Overlay::None => return,
        Overlay::Help => (
            "Help",
            vec![
                Line::from("↑/↓ navigate     Enter expand + focus     Space multi-focus"),
                Line::from("Tab switch pane  arrows orbit            drag/scroll globe"),
                Line::from("/ filter         g density               l labels"),
                Line::from("a data flow      r recenter              t trace all"),
                Line::from("s settings       d diagnostics           p auto-rotate"),
                Line::from("q quit"),
                Line::from(""),
                Line::from(Span::styled("Esc close", Style::default().fg(ACCENT))),
            ],
            68,
            13,
        ),
        Overlay::Debug => (
            "Renderer diagnostics",
            vec![
                Line::from(format!("backend       {}", app.backend.label())),
                Line::from(format!(
                    "framebuffer   {}×{}",
                    app.frame_px.0, app.frame_px.1
                )),
                Line::from(format!(
                    "terminal cell {}×{}px",
                    app.cell_px.w, app.cell_px.h
                )),
                Line::from(format!("zoom          {:.2}×", app.globe.zoom)),
                Line::from(format!("render rate   {:.0} fps", app.fps)),
                Line::from(""),
                Line::from("b braille · h halfblocks · k kitty · Esc close"),
            ],
            58,
            11,
        ),
        Overlay::Settings => {
            let setting = |on: bool| if on { "[✓]" } else { "[ ]" };
            (
                "Settings",
                vec![
                    Line::from(format!(
                        "1  {} Automatic traceroutes",
                        setting(app.snapshot.settings.traces_enabled)
                    )),
                    Line::from(format!(
                        "2  {} Local GeoIP only",
                        setting(app.snapshot.settings.geo_local_only)
                    )),
                    Line::from(format!(
                        "3  {} Record history",
                        setting(app.snapshot.settings.history_enabled)
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Press 1–3 to toggle · Esc close",
                        Style::default().fg(ACCENT),
                    )),
                ],
                58,
                9,
            )
        }
        Overlay::Privacy => (
            "Welcome · privacy",
            vec![
                Line::from("Network monitoring reads local socket tables and process metadata."),
                Line::from("Online GeoIP may send hop and destination IPs to third parties."),
                Line::from("Connection lists and process names are not uploaded."),
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        "A",
                        Style::default()
                            .fg(BG)
                            .bg(ACCENT)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" accept + online GeoIP    "),
                    Span::styled(
                        "L",
                        Style::default().fg(BG).bg(OK).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" accept + local only    Q quit"),
                ]),
            ],
            76,
            10,
        ),
    };

    let area = centered_rect(
        width.min(f.area().width.saturating_sub(4)),
        height,
        f.area(),
    );
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(format!(" {title} "))
                    .title_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(ACCENT))
                    .padding(Padding::uniform(1)),
            )
            .style(Style::default().fg(TEXT).bg(PANEL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height.min(area.height)) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

fn rgb(color: [u8; 3]) -> Color {
    Color::Rgb(color[0], color[1], color[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gfx::CellPx,
        globe::{GlobeRenderer, Hop, Path},
        mock,
    };
    use image::DynamicImage;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn wide_layout_contains_product_chrome_and_grouped_apps() {
        let globe = GlobeRenderer::from_image(DynamicImage::new_rgb8(4, 4));
        let mut app = App::new(
            mock::demo_snapshot(),
            globe,
            GfxBackend::Braille,
            CellPx::default(),
        );
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut output = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                output.push_str(buffer.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(output.contains("Network Cartographer"));
        assert!(output.contains("APPLICATIONS"));
        assert!(output.contains("Firefox"));
        assert!(output.contains("GLOBE"));
        assert!(!output.contains("framebuffer"));
    }

    #[test]
    fn narrow_layout_uses_active_pane() {
        let globe = GlobeRenderer::from_image(DynamicImage::new_rgb8(4, 4));
        let mut app = App::new(
            mock::demo_snapshot(),
            globe,
            GfxBackend::Braille,
            CellPx::default(),
        );
        app.pane = Pane::Globe;
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut output = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                output.push_str(buffer.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(output.contains("GLOBE"));
        assert!(!output.contains("APPLICATIONS"));
    }

    #[test]
    fn globe_destination_is_rendered_as_native_terminal_text() {
        let globe = GlobeRenderer::from_image(DynamicImage::new_rgb8(4, 4));
        let mut app = App::new(
            mock::demo_snapshot(),
            globe,
            GfxBackend::Braille,
            CellPx::default(),
        );
        app.pane = Pane::Globe;
        app.globe.yaw = 0.0;
        app.globe.pitch = 0.0;
        app.globe.paths = vec![Path {
            app_id: "test".into(),
            color: [34, 211, 238],
            hops: vec![Hop {
                lat: 0.0,
                lon: 0.0,
                label: "Dublin, Ireland".into(),
                show_marker: true,
            }],
        }];
        app.frame_px = (160, 96);
        app.last_frame = Some(app.globe.render(app.frame_px.0, app.frame_px.1));

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut output = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                output.push_str(buffer.cell((x, y)).unwrap().symbol());
            }
        }

        assert!(output.contains("DUB"));
    }

    #[test]
    fn kitty_does_not_render_destination_as_terminal_text() {
        let globe = GlobeRenderer::from_image(DynamicImage::new_rgb8(4, 4));
        let mut app = App::new(
            mock::demo_snapshot(),
            globe,
            GfxBackend::Kitty,
            CellPx::default(),
        );
        app.pane = Pane::Globe;
        app.globe.yaw = 0.0;
        app.globe.pitch = 0.0;
        app.globe.paths = vec![Path {
            app_id: "test".into(),
            color: [34, 211, 238],
            hops: vec![Hop {
                lat: 0.0,
                lon: 0.0,
                label: "Dublin, Ireland".into(),
                show_marker: true,
            }],
        }];
        app.frame_px = (160, 96);
        app.last_frame = Some(app.globe.render(app.frame_px.0, app.frame_px.1));

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut output = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                output.push_str(buffer.cell((x, y)).unwrap().symbol());
            }
        }

        assert!(!output.contains("DUB"));
    }
}
