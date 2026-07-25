//! Terminal pixel output: Kitty graphics, Unicode halfblocks, or Braille.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use image::RgbImage;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use std::io::{self, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GfxBackend {
    /// Kitty / Ghostty — real pixels via graphics protocol.
    Kitty,
    /// Unicode Braille (U+2800) — 2×4 dots per cell (~4× halfblocks).
    Braille,
    /// ▀ half-blocks — 1×2 “pixels” per cell.
    Halfblocks,
}

impl GfxBackend {
    pub fn detect() -> Self {
        if let Ok(v) = std::env::var("HOPGLOBE_GFX") {
            return match v.to_ascii_lowercase().as_str() {
                "kitty" | "kgp" => GfxBackend::Kitty,
                "braille" | "brl" | "dots" => GfxBackend::Braille,
                "half" | "halfblocks" | "unicode" => GfxBackend::Halfblocks,
                _ => GfxBackend::Braille,
            };
        }
        let term = std::env::var("TERM").unwrap_or_default();
        let prog = std::env::var("TERM_PROGRAM").unwrap_or_default();
        if term.contains("kitty")
            || prog.eq_ignore_ascii_case("ghostty")
            || std::env::var("KITTY_WINDOW_ID").is_ok()
            || std::env::var("GHOSTTY_RESOURCES_DIR").is_ok()
        {
            return GfxBackend::Kitty;
        }
        // Portable default: braille is denser than halfblocks on any truecolor term.
        GfxBackend::Braille
    }

    pub fn label(self) -> &'static str {
        match self {
            GfxBackend::Kitty => "kitty-graphics",
            GfxBackend::Braille => "braille",
            GfxBackend::Halfblocks => "halfblocks",
        }
    }
}

/// Font cell size in pixels (width × height).
#[derive(Clone, Copy, Debug)]
pub struct CellPx {
    pub w: u32,
    pub h: u32,
}

impl Default for CellPx {
    fn default() -> Self {
        Self { w: 10, h: 20 }
    }
}

/// Detect character cell size in pixels via crossterm `window_size` (TIOCGWINSZ).
pub fn detect_cell_px() -> CellPx {
    if let Ok(ws) = crossterm::terminal::window_size() {
        let cols = ws.columns as u32;
        let rows = ws.rows as u32;
        let px = ws.width as u32;
        let py = ws.height as u32;
        if cols > 0 && rows > 0 && px >= cols && py >= rows {
            let w = (px / cols).max(1);
            let h = (py / rows).max(1);
            if w >= 4 && w <= 64 && h >= 6 && h <= 128 {
                return CellPx { w, h };
            }
        }
    }
    CellPx::default()
}

/// Pixel size of a terminal rectangle so the framebuffer matches on-screen aspect.
///
/// - Halfblocks: 1×2 logical px/cell
/// - Braille: 2×4 logical px/cell
/// - Kitty: physical (cols·cell_w)×(rows·cell_h), capped
pub fn framebuffer_size(
    backend: GfxBackend,
    cols: u16,
    rows: u16,
    cell: CellPx,
) -> (u32, u32) {
    let cols = cols.max(2) as u32;
    let rows = rows.max(2) as u32;
    match backend {
        GfxBackend::Halfblocks => (cols, rows * 2),
        GfxBackend::Braille => (cols * 2, rows * 4),
        GfxBackend::Kitty => {
            let phys_w = cols.saturating_mul(cell.w).max(1);
            let phys_h = rows.saturating_mul(cell.h).max(1);
            let aspect = phys_w as f32 / phys_h as f32;

            let max_dim = 480u32;
            let (pw, ph) = if phys_w >= phys_h {
                let pw = phys_w.min(max_dim);
                let ph = ((pw as f32) / aspect).round().max(32.0) as u32;
                (pw.max(32), ph)
            } else {
                let ph = phys_h.min(max_dim);
                let pw = ((ph as f32) * aspect).round().max(32.0) as u32;
                (pw, ph.max(32))
            };
            (pw, ph)
        }
    }
}

/// Paint an RGB image into a ratatui area using ▀ half-blocks.
pub struct HalfblockImage<'a> {
    pub img: &'a RgbImage,
}

impl Widget for HalfblockImage<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let iw = self.img.width().max(1);
        let ih = self.img.height().max(1);

        for row in 0..area.height {
            for col in 0..area.width {
                let x = sample_x(col, area.width, iw);
                let y_top = sample_y(row * 2, area.height * 2, ih);
                let y_bot = sample_y(row * 2 + 1, area.height * 2, ih);
                let top = punch_rgb(self.img.get_pixel(x, y_top).0);
                let bot = punch_rgb(self.img.get_pixel(x, y_bot).0);
                if let Some(cell) = buf.cell_mut((area.x + col, area.y + row)) {
                    cell.set_symbol("▀");
                    cell.set_style(
                        Style::default()
                            .fg(Color::Rgb(top[0], top[1], top[2]))
                            .bg(Color::Rgb(bot[0], bot[1], bot[2])),
                    );
                }
            }
        }
    }
}

/// Paint an RGB image using Unicode Braille (2×4 dots per cell).
///
/// Dot numbering (Unicode braille):
/// ```text
///  1 4
///  2 5
///  3 6
///  7 8
/// ```
pub struct BrailleImage<'a> {
    pub img: &'a RgbImage,
}

impl Widget for BrailleImage<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let iw = self.img.width().max(1);
        let ih = self.img.height().max(1);
        // Logical grid: 2 px wide × 4 px tall per cell.
        let lw = area.width as u32 * 2;
        let lh = area.height as u32 * 4;

        // Braille bit for each (dx, dy) in the 2×4 cell.
        // order: (0,0)=1, (0,1)=2, (0,2)=3, (1,0)=4, (1,1)=5, (1,2)=6, (0,3)=7, (1,3)=8
        const DOT_BITS: [[u8; 4]; 2] = [
            [0x01, 0x02, 0x04, 0x40], // col 0: dots 1,2,3,7
            [0x08, 0x10, 0x20, 0x80], // col 1: dots 4,5,6,8
        ];

        for row in 0..area.height {
            for col in 0..area.width {
                let mut samples = [[(0u8, 0u8, 0u8, 0u16); 4]; 2]; // r,g,b,luma
                let mut sum_luma = 0u32;

                for dy in 0..4u16 {
                    for dx in 0..2u16 {
                        let lx = col as u32 * 2 + dx as u32;
                        let ly = row as u32 * 4 + dy as u32;
                        let x = sample_x_from_logical(lx, lw, iw);
                        let y = sample_y_from_logical(ly, lh, ih);
                        let p = self.img.get_pixel(x, y).0;
                        let luma = luma_u16(p);
                        samples[dx as usize][dy as usize] = (p[0], p[1], p[2], luma);
                        sum_luma += luma as u32;
                    }
                }

                // Slightly above mean → land/paths light, oceans empty more often.
                let mean = (sum_luma / 8) as u16;
                let threshold = mean.saturating_add(14).min(210);

                let mut mask: u8 = 0;
                let mut on_r = 0u32;
                let mut on_g = 0u32;
                let mut on_b = 0u32;
                let mut on_n = 0u32;
                let mut off_r = 0u32;
                let mut off_g = 0u32;
                let mut off_b = 0u32;
                let mut off_n = 0u32;

                for dy in 0..4usize {
                    for dx in 0..2usize {
                        let (r, g, b, luma) = samples[dx][dy];
                        // Favor lit dots on brighter samples for clearer continent edges.
                        let on = luma >= threshold
                            || (mean < 36 && luma + 18 >= mean)
                            || luma >= 140; // always show bright path/marker pixels
                        if on {
                            mask |= DOT_BITS[dx][dy];
                            on_r += r as u32;
                            on_g += g as u32;
                            on_b += b as u32;
                            on_n += 1;
                        } else {
                            off_r += r as u32;
                            off_g += g as u32;
                            off_b += b as u32;
                            off_n += 1;
                        }
                    }
                }

                // If nothing lit (very dark), solid near-black ocean cell.
                if mask == 0 {
                    let (br, bgc, bb) = if off_n > 0 {
                        // Crush dark cells so they don't wash out lit dots nearby.
                        (
                            (off_r / off_n).saturating_mul(70) / 100,
                            (off_g / off_n).saturating_mul(70) / 100,
                            (off_b / off_n).saturating_mul(75) / 100,
                        )
                    } else {
                        (2, 3, 8)
                    };
                    if let Some(cell) = buf.cell_mut((area.x + col, area.y + row)) {
                        cell.set_symbol("\u{2800}");
                        cell.set_style(
                            Style::default()
                                .fg(Color::Rgb(br as u8, bgc as u8, bb as u8))
                                .bg(Color::Rgb(br as u8, bgc as u8, bb as u8)),
                        );
                    }
                    continue;
                }

                let (fr, fg, fb) = boost_cell_fg(on_r / on_n, on_g / on_n, on_b / on_n);
                let (br, bgc, bb) = if off_n > 0 {
                    // Darker bg → higher fg/bg contrast for braille dots.
                    (
                        (off_r / off_n).saturating_mul(55) / 100,
                        (off_g / off_n).saturating_mul(55) / 100,
                        (off_b / off_n).saturating_mul(60) / 100,
                    )
                } else {
                    (
                        fr.saturating_mul(25) / 100,
                        fg.saturating_mul(25) / 100,
                        fb.saturating_mul(25) / 100,
                    )
                };

                let ch = char::from_u32(0x2800 + mask as u32).unwrap_or('\u{2800}');
                let mut tmp = [0u8; 4];
                let sym = ch.encode_utf8(&mut tmp);
                if let Some(cell) = buf.cell_mut((area.x + col, area.y + row)) {
                    cell.set_symbol(sym);
                    cell.set_style(
                        Style::default()
                            .fg(Color::Rgb(fr as u8, fg as u8, fb as u8))
                            .bg(Color::Rgb(br as u8, bgc as u8, bb as u8)),
                    );
                }
            }
        }
    }
}

#[inline]
fn luma_u16(p: [u8; 3]) -> u16 {
    // Rec. 601-ish
    ((p[0] as u16 * 30 + p[1] as u16 * 59 + p[2] as u16 * 11) / 100) as u16
}

/// Brighten / saturate FG so braille dots pop against a crushed BG.
fn boost_cell_fg(r: u32, g: u32, b: u32) -> (u32, u32, u32) {
    let r = (r as f32 * 1.25 + 18.0).min(255.0);
    let g = (g as f32 * 1.25 + 18.0).min(255.0);
    let b = (b as f32 * 1.25 + 18.0).min(255.0);
    let y = 0.30 * r + 0.59 * g + 0.11 * b;
    let sat = 1.4;
    (
        (y + (r - y) * sat).clamp(0.0, 255.0) as u32,
        (y + (g - y) * sat).clamp(0.0, 255.0) as u32,
        (y + (b - y) * sat).clamp(0.0, 255.0) as u32,
    )
}

fn punch_rgb(p: [u8; 3]) -> [u8; 3] {
    let r = (p[0] as f32 * 1.12 + 8.0).min(255.0);
    let g = (p[1] as f32 * 1.12 + 8.0).min(255.0);
    let b = (p[2] as f32 * 1.12 + 8.0).min(255.0);
    let y = 0.30 * r + 0.59 * g + 0.11 * b;
    let sat = 1.28;
    [
        (y + (r - y) * sat).clamp(0.0, 255.0) as u8,
        (y + (g - y) * sat).clamp(0.0, 255.0) as u8,
        (y + (b - y) * sat).clamp(0.0, 255.0) as u8,
    ]
}

#[inline]
fn sample_x(col: u16, area_w: u16, img_w: u32) -> u32 {
    if img_w == area_w as u32 {
        (col as u32).min(img_w - 1)
    } else {
        ((col as u32 * img_w) / area_w as u32).min(img_w - 1)
    }
}

#[inline]
fn sample_y(row: u16, area_h: u16, img_h: u32) -> u32 {
    if img_h == area_h as u32 {
        (row as u32).min(img_h - 1)
    } else {
        ((row as u32 * img_h) / area_h as u32).min(img_h - 1)
    }
}

#[inline]
fn sample_x_from_logical(lx: u32, logical_w: u32, img_w: u32) -> u32 {
    if img_w == logical_w {
        lx.min(img_w - 1)
    } else {
        ((lx * img_w) / logical_w.max(1)).min(img_w - 1)
    }
}

#[inline]
fn sample_y_from_logical(ly: u32, logical_h: u32, img_h: u32) -> u32 {
    if img_h == logical_h {
        ly.min(img_h - 1)
    } else {
        ((ly * img_h) / logical_h.max(1)).min(img_h - 1)
    }
}

pub fn mark_skip(area: Rect, buf: &mut Buffer) {
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.reset();
                cell.set_skip(true);
            }
        }
    }
}

pub fn sync_begin(out: &mut impl Write) -> io::Result<()> {
    write!(out, "\x1b[?2026h")
}

pub fn sync_end(out: &mut impl Write) -> io::Result<()> {
    write!(out, "\x1b[?2026l")
}

pub fn kitty_place_rgb(img: &RgbImage, cols: u16, rows: u16, image_id: u32) -> io::Result<()> {
    let w = img.width();
    let h = img.height();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for p in img.pixels() {
        rgb.extend_from_slice(&p.0);
    }
    let encoded = B64.encode(&rgb);
    let mut out = io::stdout().lock();

    const CHUNK: usize = 4096;
    let bytes = encoded.as_bytes();
    let n = bytes.len().div_ceil(CHUNK);
    for (i, chunk) in bytes.chunks(CHUNK).enumerate() {
        let more = if i + 1 < n { 1 } else { 0 };
        let chunk = std::str::from_utf8(chunk).unwrap();
        if i == 0 {
            write!(
                out,
                "\x1b_Ga=T,f=24,t=d,q=2,C=1,i={image_id},s={w},v={h},c={cols},r={rows},m={more};{chunk}\x1b\\"
            )?;
        } else {
            write!(out, "\x1b_Gm={more};{chunk}\x1b\\")?;
        }
    }
    out.flush()?;
    Ok(())
}

pub fn kitty_delete(image_id: u32) -> io::Result<()> {
    let mut out = io::stdout().lock();
    write!(out, "\x1b_Ga=d,d=i,q=2,i={image_id}\x1b\\")?;
    out.flush()
}
