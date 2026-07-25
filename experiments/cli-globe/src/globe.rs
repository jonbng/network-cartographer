//! Software 3D globe: equirectangular texture on a sphere, great-circle arcs, hop dots.
//! Renders into an RGB framebuffer — no WebGL, no HTML.

use image::{DynamicImage, Rgb, RgbImage};
use std::collections::BTreeSet;

const DEST_COLOR: [u8; 3] = [249, 168, 212];
const DIM_COLOR: [u8; 3] = [62, 76, 94];

#[derive(Clone, Debug)]
pub struct Hop {
    pub lat: f32,
    pub lon: f32,
    pub label: String,
    pub show_marker: bool,
}

#[derive(Clone, Debug)]
pub struct Path {
    pub app_id: String,
    pub color: [u8; 3],
    pub hops: Vec<Hop>,
}

#[derive(Clone, Debug)]
pub struct LabelAnchor {
    pub x: f32,
    pub y: f32,
    pub text: String,
}

pub struct GlobeRenderer {
    tex: RgbImage,
    tex_w: u32,
    tex_h: u32,
    pub yaw: f32,
    pub pitch: f32,
    /// 1.0 ≈ whole globe in frame; higher = tighter on the surface.
    pub zoom: f32,
    pub paths: Vec<Path>,
    pub focused_apps: BTreeSet<String>,
    pub show_labels: bool,
    /// Normalized packet position, advanced by the terminal event loop.
    pub flow_phase: f32,
    /// Mosaic backends need single-sample endpoints; Kitty can afford rings.
    pub compact_destination_markers: bool,
}

impl GlobeRenderer {
    pub const ZOOM_MIN: f32 = 0.82;
    pub const ZOOM_MAX: f32 = 10.0;
    /// Halfblocks: tight on the map (low pixel density needs the close-up).
    pub const ZOOM_DEFAULT_HALF: f32 = 1.08;
    /// Braille: denser than halfblocks, so slightly more zoomed out.
    pub const ZOOM_DEFAULT_BRAILLE: f32 = 0.96;
    /// Kitty/pixel: more zoomed out so the whole globe reads clearly.
    pub const ZOOM_DEFAULT_KITTY: f32 = 0.92;
    /// Generic default.
    pub const ZOOM_DEFAULT: f32 = Self::ZOOM_DEFAULT_BRAILLE;

    pub fn from_image(img: DynamicImage) -> Self {
        let rgb = img.to_rgb8();
        let (tex_w, tex_h) = rgb.dimensions();
        Self {
            tex: rgb,
            tex_w,
            tex_h,
            // Face northern Europe / N Atlantic (matches demo hop cluster).
            yaw: 0.15,
            pitch: 0.55,
            zoom: Self::ZOOM_DEFAULT,
            paths: Vec::new(),
            focused_apps: BTreeSet::new(),
            show_labels: true,
            flow_phase: 0.0,
            compact_destination_markers: true,
        }
    }

    pub fn default_zoom_for_backend(kitty: bool, braille: bool) -> f32 {
        if kitty {
            Self::ZOOM_DEFAULT_KITTY
        } else if braille {
            Self::ZOOM_DEFAULT_BRAILLE
        } else {
            Self::ZOOM_DEFAULT_HALF
        }
    }

    pub fn set_zoom(&mut self, z: f32) {
        self.zoom = z.clamp(Self::ZOOM_MIN, Self::ZOOM_MAX);
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.set_zoom(self.zoom * factor);
    }

    /// Center the camera on the spherical mean of all mapped hops while
    /// keeping the entire globe visible. This mirrors desktop's one-shot
    /// framing without fighting later manual camera movement.
    pub fn frame_paths(&mut self, zoom: f32) {
        let mut sum = [0.0f32; 3];
        let mut count = 0usize;
        for hop in self.paths.iter().flat_map(|path| &path.hops) {
            let lat = hop.lat.to_radians();
            let lon = hop.lon.to_radians();
            sum[0] += lat.cos() * lon.sin();
            sum[1] += lat.sin();
            sum[2] += lat.cos() * lon.cos();
            count += 1;
        }
        if count > 0 {
            self.yaw = sum[0].atan2(sum[2]);
            self.pitch = sum[1]
                .atan2((sum[0] * sum[0] + sum[2] * sum[2]).sqrt())
                .clamp(-1.2, 1.2);
        }
        self.set_zoom(zoom);
    }

    /// Project visible destination labels into framebuffer coordinates. The
    /// TUI converts these to terminal cells and renders native terminal text.
    pub fn label_anchors(&self, w: u32, h: u32) -> Vec<LabelAnchor> {
        if !self.show_labels || w == 0 || h == 0 {
            return Vec::new();
        }
        let cx = (w as f32 - 1.0) * 0.5;
        let cy = (h as f32 - 1.0) * 0.5;
        let r = (w.min(h) as f32) * 0.5 * self.zoom.clamp(Self::ZOOM_MIN, Self::ZOOM_MAX);
        let (cyaw, syaw) = (self.yaw.cos(), self.yaw.sin());
        let (cpitch, spitch) = (self.pitch.cos(), self.pitch.sin());

        self.paths
            .iter()
            .filter(|path| self.focused_apps.is_empty() || self.focused_apps.contains(&path.app_id))
            .filter_map(|path| {
                let destination = path.hops.last()?;
                let (x, y, _depth, front) = self.project(
                    destination.lat,
                    destination.lon,
                    cx,
                    cy,
                    r,
                    cyaw,
                    syaw,
                    cpitch,
                    spitch,
                )?;
                if !front || x < 0.0 || y < 0.0 || x >= w as f32 || y >= h as f32 {
                    return None;
                }
                let text = short_label(&destination.label);
                (!text.is_empty()).then_some(LabelAnchor { x, y, text })
            })
            .take(10)
            .collect()
    }

    pub fn sample_tex(&self, lat: f32, lon: f32) -> Rgb<u8> {
        // lat [-90,90] lon [-180,180] → UV
        let u = (lon + 180.0) / 360.0;
        let v = (90.0 - lat) / 180.0;
        let x = ((u.rem_euclid(1.0)) * self.tex_w as f32) as u32 % self.tex_w;
        let y = (v.clamp(0.0, 0.999) * self.tex_h as f32) as u32;
        let c = *self.tex.get_pixel(x, y);
        Rgb(boost_contrast(c.0))
    }

    /// Render orthographic globe into an RGB buffer of size (w, h).
    ///
    /// Assumes buffer pixels are roughly square in *display* space (caller
    /// sizes w×h for halfblocks / kitty cell aspect). Sphere uses isotropic
    /// radius so it stays circular, not stretched.
    pub fn render(&self, w: u32, h: u32) -> RgbImage {
        let mut out = RgbImage::new(w, h);
        let cx = (w as f32 - 1.0) * 0.5;
        let cy = (h as f32 - 1.0) * 0.5;
        // zoom 1 → globe fits; zoom >1 → surface fills the view (clipped).
        let r = (w.min(h) as f32) * 0.5 * self.zoom.clamp(Self::ZOOM_MIN, Self::ZOOM_MAX);

        let cyaw = self.yaw.cos();
        let syaw = self.yaw.sin();
        let cpitch = self.pitch.cos();
        let spitch = self.pitch.sin();

        // Near-black void so the globe pops harder.
        for p in out.pixels_mut() {
            *p = Rgb([2, 3, 8]);
        }

        // Sphere (orthographic, view = +Z toward camera after rotation)
        for y in 0..h {
            for x in 0..w {
                let nx = (x as f32 - cx) / r;
                let ny = (cy - y as f32) / r; // y up
                let d2 = nx * nx + ny * ny;
                if d2 > 1.0 {
                    continue;
                }
                let nz = (1.0 - d2).sqrt(); // facing camera

                // Inverse rotate view-space → world
                // pitch about X, then yaw about Y (applied in reverse)
                let (x1, y1, z1) = (nx, ny * cpitch + nz * spitch, -ny * spitch + nz * cpitch);
                let (wx, wy, wz) = (x1 * cyaw + z1 * syaw, y1, -x1 * syaw + z1 * cyaw);

                let lat = wy.asin().to_degrees();
                let lon = wx.atan2(wz).to_degrees();

                let mut c = self.sample_tex(lat, lon);
                // Higher-contrast lighting: darker terminator, brighter face.
                let ndotl = (nx * 0.25 + ny * 0.35 + nz * 0.95).clamp(0.0, 1.0);
                let light = 0.22 + 0.88 * ndotl.powf(0.85);
                c.0 = [
                    (c.0[0] as f32 * light).min(255.0) as u8,
                    (c.0[1] as f32 * light).min(255.0) as u8,
                    (c.0[2] as f32 * light).min(255.0) as u8,
                ];
                // Soft cyan rim only at the limb (helps silhouette without washing detail).
                let rim = (1.0 - nz).powi(4) * (1.2 / self.zoom).clamp(0.0, 1.0);
                c.0[0] = c.0[0].saturating_add((28.0 * rim) as u8);
                c.0[1] = c.0[1].saturating_add((70.0 * rim) as u8);
                c.0[2] = c.0[2].saturating_add((110.0 * rim) as u8);
                out.put_pixel(x, y, c);
            }
        }

        // Thin arcs work better on halfblocks (thick strokes turn into fat blobs).
        // Keep markers a touch larger so hops stay visible when zoomed.
        let stroke = 1;
        let hop_r = (1.5 + self.zoom * 0.25).round().clamp(1.0, 4.0) as i32;
        let dest_r = hop_r + 1;

        // Great-circle arcs + hop markers (project world → screen)
        let paths = self.paths.clone();
        for (path_index, path) in paths.iter().enumerate() {
            let dimmed = !self.focused_apps.is_empty() && !self.focused_apps.contains(&path.app_id);
            let path_color = if dimmed { DIM_COLOR } else { path.color };
            // Dense great-circle samples look better when zoomed (less chordy).
            for pair in path.hops.windows(2) {
                let a = &pair[0];
                let b = &pair[1];
                let steps = (12.0 + self.zoom * 8.0).round() as i32;
                let mut prev: Option<(f32, f32, bool)> = None;
                for s in 0..=steps {
                    let t = s as f32 / steps as f32;
                    let (lat, lon) = great_circle_interp(a.lat, a.lon, b.lat, b.lon, t);
                    if let Some((sx, sy, _d, front)) =
                        self.project(lat, lon, cx, cy, r, cyaw, syaw, cpitch, spitch)
                    {
                        if sx < -8.0 || sy < -8.0 || sx > w as f32 + 8.0 || sy > h as f32 + 8.0 {
                            prev = None;
                            continue;
                        }
                        if let Some((px, py, pfront)) = prev {
                            if front || pfront {
                                draw_line(
                                    &mut out, px as i32, py as i32, sx as i32, sy as i32,
                                    path_color, stroke,
                                );
                            }
                        }
                        prev = Some((sx, sy, front));
                    } else {
                        prev = None;
                    }
                }
            }

            // One small packet and two fading trail dots move from the first
            // hop toward the destination. A per-route offset avoids a wall of
            // synchronized particles while preserving direction.
            if !dimmed && path.hops.len() > 1 {
                let segment_count = path.hops.len() - 1;
                let route_phase = (self.flow_phase + path_index as f32 * 0.173).fract();
                for trail in (0..3).rev() {
                    let lag = trail as f32 * 0.018;
                    if route_phase < lag {
                        continue;
                    }
                    let phase = route_phase - lag;
                    let progress = phase * segment_count as f32;
                    let segment = (progress.floor() as usize).min(segment_count - 1);
                    let local_t = progress - segment as f32;
                    let a = &path.hops[segment];
                    let b = &path.hops[segment + 1];
                    let (lat, lon) = great_circle_interp(a.lat, a.lon, b.lat, b.lon, local_t);
                    if let Some((sx, sy, _depth, front)) =
                        self.project(lat, lon, cx, cy, r, cyaw, syaw, cpitch, spitch)
                    {
                        if front && sx >= 0.0 && sy >= 0.0 && sx < w as f32 && sy < h as f32 {
                            let radius = if trail == 0 {
                                hop_r.max(2) as f32 * 0.8
                            } else {
                                0.8
                            };
                            fill_circle_fractional(
                                &mut out,
                                sx as i32,
                                sy as i32,
                                radius,
                                flow_color(path.color, trail),
                            );
                        }
                    }
                }
            }

            for (i, hop) in path.hops.iter().enumerate() {
                if let Some((sx, sy, _d, front)) =
                    self.project(hop.lat, hop.lon, cx, cy, r, cyaw, syaw, cpitch, spitch)
                {
                    if !front || !hop.show_marker {
                        continue;
                    }
                    if sx < 0.0 || sy < 0.0 || sx >= w as f32 || sy >= h as f32 {
                        continue;
                    }
                    let dest = i + 1 == path.hops.len();
                    let rad = if dest && self.compact_destination_markers {
                        1
                    } else if dest {
                        dest_r
                    } else {
                        hop_r
                    };
                    let marker_color = if dimmed {
                        DIM_COLOR
                    } else if dest {
                        DEST_COLOR
                    } else {
                        path.color
                    };
                    fill_circle(&mut out, sx as i32, sy as i32, rad, marker_color);
                    if dest && !self.compact_destination_markers {
                        draw_circle(&mut out, sx as i32, sy as i32, rad + 1, marker_color);
                    }
                }
            }
        }

        // Kitty receives the framebuffer as real pixels, so keep its labels
        // inside that image. Mosaic backends use native terminal text instead.
        if self.show_labels && !self.compact_destination_markers {
            draw_pixel_labels(&mut out, &self.label_anchors(w, h));
        }

        out
    }

    fn project(
        &self,
        lat: f32,
        lon: f32,
        cx: f32,
        cy: f32,
        r: f32,
        cyaw: f32,
        syaw: f32,
        cpitch: f32,
        spitch: f32,
    ) -> Option<(f32, f32, f32, bool)> {
        let lat = lat.to_radians();
        let lon = lon.to_radians();
        let wx = lat.cos() * lon.sin();
        let wy = lat.sin();
        let wz = lat.cos() * lon.cos();

        // world → view (yaw then pitch)
        let x1 = wx * cyaw - wz * syaw;
        let z1 = wx * syaw + wz * cyaw;
        let y1 = wy;
        let vx = x1;
        let vy = y1 * cpitch - z1 * spitch;
        let vz = y1 * spitch + z1 * cpitch;

        // orthographic: only front hemisphere
        let front = vz > -0.05;
        let sx = cx + vx * r;
        let sy = cy - vy * r;
        Some((sx, sy, vz, front))
    }
}

/// The shared desktop texture is intentionally near-black (0–40 RGB). Expand
/// that narrow range into a cool blue-gray globe so continents remain legible
/// after sphere lighting and terminal downsampling.
fn boost_contrast(rgb: [u8; 3]) -> [u8; 3] {
    let luma = (rgb[0] as f32 * 0.30 + rgb[1] as f32 * 0.59 + rgb[2] as f32 * 0.11) / 255.0;
    let value = (luma * 5.2).clamp(0.0, 1.0).powf(0.82);
    [
        (value * 82.0).round() as u8,
        (value * 104.0).round() as u8,
        (value * 148.0).round() as u8,
    ]
}

/// Path/hop colors: punchy and bright for terminal mosaics.
pub fn punch_color(rgb: [u8; 3]) -> [u8; 3] {
    let r = (rgb[0] as f32 * 1.15 + 20.0).min(255.0) as u8;
    let g = (rgb[1] as f32 * 1.15 + 20.0).min(255.0) as u8;
    let b = (rgb[2] as f32 * 1.15 + 20.0).min(255.0) as u8;
    // Keep chroma high vs gray.
    let y = (r as u16 + g as u16 + b as u16) / 3;
    [
        r.saturating_add(((r as i16 - y as i16) / 4).max(0) as u8),
        g.saturating_add(((g as i16 - y as i16) / 4).max(0) as u8),
        b.saturating_add(((b as i16 - y as i16) / 4).max(0) as u8),
    ]
}

fn flow_color(path: [u8; 3], trail: usize) -> [u8; 3] {
    let white_mix = match trail {
        0 => 0.82,
        1 => 0.48,
        _ => 0.22,
    };
    let mix = |channel: u8| {
        (channel as f32 * (1.0 - white_mix) + 255.0 * white_mix)
            .round()
            .min(255.0) as u8
    };
    [mix(path[0]), mix(path[1]), mix(path[2])]
}

/// Spherical linear-ish interp on the globe surface (lat/lon degrees).
fn great_circle_interp(lat0: f32, lon0: f32, lat1: f32, lon1: f32, t: f32) -> (f32, f32) {
    let φ1 = lat0.to_radians();
    let λ1 = lon0.to_radians();
    let φ2 = lat1.to_radians();
    let λ2 = lon1.to_radians();
    let x1 = φ1.cos() * λ1.cos();
    let y1 = φ1.cos() * λ1.sin();
    let z1 = φ1.sin();
    let x2 = φ2.cos() * λ2.cos();
    let y2 = φ2.cos() * λ2.sin();
    let z2 = φ2.sin();
    let dot = (x1 * x2 + y1 * y2 + z1 * z2).clamp(-1.0, 1.0);
    let ω = dot.acos();
    if ω < 1e-5 {
        return (lat0 + (lat1 - lat0) * t, lon0 + (lon1 - lon0) * t);
    }
    let sin_ω = ω.sin();
    let a = ((1.0 - t) * ω).sin() / sin_ω;
    let b = (t * ω).sin() / sin_ω;
    let x = a * x1 + b * x2;
    let y = a * y1 + b * y2;
    let z = a * z1 + b * z2;
    let lat = z.atan2((x * x + y * y).sqrt()).to_degrees();
    let lon = y.atan2(x).to_degrees();
    (lat, lon)
}

fn short_label(label: &str) -> String {
    let clean = label
        .split(',')
        .next()
        .unwrap_or(label)
        .trim()
        .to_ascii_uppercase();
    let words: Vec<&str> = clean
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    if words.len() > 1 {
        words
            .iter()
            .filter_map(|word| word.chars().next())
            .take(3)
            .collect()
    } else {
        clean
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .take(3)
            .collect()
    }
}

/// Draw compact 5×7 glyphs into Kitty's pixel framebuffer. At one framebuffer
/// pixel per dot these stay below terminal-font size while remaining legible.
fn draw_pixel_labels(img: &mut RgbImage, anchors: &[LabelAnchor]) {
    const GLYPH_WIDTH: i32 = 5;
    const GLYPH_HEIGHT: i32 = 7;
    const TRACKING: i32 = 1;
    const PADDING: i32 = 1;

    let image_width = img.width() as i32;
    let image_height = img.height() as i32;
    let mut occupied: Vec<(i32, i32, i32, i32)> = Vec::new();

    for anchor in anchors {
        let glyph_count = anchor.text.chars().count() as i32;
        if glyph_count == 0 {
            continue;
        }
        let text_width = glyph_count * (GLYPH_WIDTH + TRACKING) - TRACKING;
        let box_width = text_width + PADDING * 2;
        let box_height = GLYPH_HEIGHT + PADDING * 2;
        let anchor_x = anchor.x.round() as i32;
        let anchor_y = anchor.y.round() as i32;
        let right = anchor_x + 4;
        let left = anchor_x - box_width - 4;

        let placement = [-box_height / 2, -box_height - 3, 3]
            .into_iter()
            .flat_map(|y_offset| [(right, anchor_y + y_offset), (left, anchor_y + y_offset)])
            .find(|&(x, y)| {
                let candidate = (x, y, box_width, box_height);
                x >= 0
                    && y >= 0
                    && x + box_width <= image_width
                    && y + box_height <= image_height
                    && occupied
                        .iter()
                        .all(|&other| !pixel_rects_intersect(candidate, other))
            });
        let Some((x, y)) = placement else { continue };
        occupied.push((x, y, box_width, box_height));

        blend_rect(img, x, y, box_width, box_height, [3, 7, 13], 0.82);
        let mut pen_x = x + PADDING;
        for ch in anchor.text.chars() {
            draw_glyph_5x7(img, pen_x, y + PADDING, ch, DEST_COLOR);
            pen_x += GLYPH_WIDTH + TRACKING;
        }
    }
}

fn pixel_rects_intersect(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0 < b.0 + b.2 && a.0 + a.2 > b.0 && a.1 < b.1 + b.3 && a.1 + a.3 > b.1
}

fn blend_rect(
    img: &mut RgbImage,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    color: [u8; 3],
    opacity: f32,
) {
    for py in y..y + height {
        for px in x..x + width {
            let existing = img.get_pixel(px as u32, py as u32).0;
            let blend = |from: u8, to: u8| {
                (from as f32 * (1.0 - opacity) + to as f32 * opacity).round() as u8
            };
            img.put_pixel(
                px as u32,
                py as u32,
                Rgb([
                    blend(existing[0], color[0]),
                    blend(existing[1], color[1]),
                    blend(existing[2], color[2]),
                ]),
            );
        }
    }
}

fn draw_glyph_5x7(img: &mut RgbImage, x: i32, y: i32, ch: char, color: [u8; 3]) {
    let glyph = glyph_5x7(ch);
    for (row, bits) in glyph.into_iter().enumerate() {
        for column in 0..5 {
            if bits & (1 << (4 - column)) != 0 {
                img.put_pixel((x + column) as u32, (y + row as i32) as u32, Rgb(color));
            }
        }
    }
}

fn glyph_5x7(ch: char) -> [u8; 7] {
    match ch {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        _ => [0; 7],
    }
}

fn fill_circle(img: &mut RgbImage, cx: i32, cy: i32, rad: i32, color: [u8; 3]) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let color = punch_color(color);
    for dy in -rad..=rad {
        for dx in -rad..=rad {
            if dx * dx + dy * dy <= rad * rad {
                let x = cx + dx;
                let y = cy + dy;
                if x >= 0 && y >= 0 && x < w && y < h {
                    img.put_pixel(x as u32, y as u32, Rgb(color));
                }
            }
        }
    }
}

fn fill_circle_fractional(img: &mut RgbImage, cx: i32, cy: i32, radius: f32, color: [u8; 3]) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let color = punch_color(color);
    let extent = radius.ceil() as i32;
    let radius_sq = radius * radius;
    for dy in -extent..=extent {
        for dx in -extent..=extent {
            if (dx * dx + dy * dy) as f32 <= radius_sq {
                let x = cx + dx;
                let y = cy + dy;
                if x >= 0 && y >= 0 && x < w && y < h {
                    img.put_pixel(x as u32, y as u32, Rgb(color));
                }
            }
        }
    }
}

fn draw_circle(img: &mut RgbImage, cx: i32, cy: i32, rad: i32, color: [u8; 3]) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    for a in 0..64 {
        let t = a as f32 * std::f32::consts::TAU / 64.0;
        let x = cx + (t.cos() * rad as f32) as i32;
        let y = cy + (t.sin() * rad as f32) as i32;
        if x >= 0 && y >= 0 && x < w && y < h {
            img.put_pixel(x as u32, y as u32, Rgb(color));
        }
    }
}

fn draw_line(
    img: &mut RgbImage,
    mut x0: i32,
    mut y0: i32,
    x1: i32,
    y1: i32,
    color: [u8; 3],
    thick: i32,
) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let color = punch_color(color);
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    // Single-pixel strokes: only the center pixel (no ±thick vertical fattening).
    let half = (thick - 1).max(0) / 2;
    loop {
        for t in -half..=half {
            let x = x0;
            let y = y0 + t;
            if x >= 0 && y >= 0 && x < w && y < h {
                // Prefer path color over terrain so arcs stay readable.
                let p = img.get_pixel_mut(x as u32, y as u32);
                p.0[0] = ((p.0[0] as u16 + color[0] as u16 * 4) / 5) as u8;
                p.0[1] = ((p.0[1] as u16 + color[1] as u16 * 4) / 5) as u8;
                p.0[2] = ((p.0[2] as u16 + color[2] as u16 * 4) / 5) as u8;
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_phase_moves_the_route_packet() {
        let mut globe = GlobeRenderer::from_image(DynamicImage::new_rgb8(8, 4));
        globe.paths = vec![Path {
            app_id: "test".into(),
            color: [34, 211, 238],
            hops: vec![
                Hop {
                    lat: 0.0,
                    lon: -35.0,
                    label: "A".into(),
                    show_marker: true,
                },
                Hop {
                    lat: 10.0,
                    lon: 35.0,
                    label: "B".into(),
                    show_marker: true,
                },
            ],
        }];
        globe.yaw = 0.0;
        globe.pitch = 0.0;
        globe.flow_phase = 0.1;
        let first = globe.render(160, 120);
        globe.flow_phase = 0.6;
        let second = globe.render(160, 120);
        assert_ne!(first.as_raw(), second.as_raw());
    }

    #[test]
    fn label_anchors_use_short_visible_destination_names() {
        let mut globe = GlobeRenderer::from_image(DynamicImage::new_rgb8(8, 4));
        globe.yaw = 0.0;
        globe.pitch = 0.0;
        globe.paths = vec![Path {
            app_id: "test".into(),
            color: [34, 211, 238],
            hops: vec![Hop {
                lat: 0.0,
                lon: 0.0,
                label: "Dublin, Ireland".into(),
                show_marker: true,
            }],
        }];

        let anchors = globe.label_anchors(160, 120);

        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].text, "DUB");
        assert!(anchors[0].x > 70.0 && anchors[0].x < 90.0);
        assert!(anchors[0].y > 50.0 && anchors[0].y < 70.0);
    }

    #[test]
    fn kitty_composites_labels_into_the_pixel_framebuffer_only() {
        let mut globe = GlobeRenderer::from_image(DynamicImage::new_rgb8(8, 4));
        globe.yaw = 0.0;
        globe.pitch = 0.0;
        globe.compact_destination_markers = false;
        globe.paths = vec![Path {
            app_id: "test".into(),
            color: [34, 211, 238],
            hops: vec![Hop {
                lat: 0.0,
                lon: 0.0,
                label: "Dublin, Ireland".into(),
                show_marker: true,
            }],
        }];

        globe.show_labels = false;
        let without_labels = globe.render(160, 120);
        globe.show_labels = true;
        let kitty_labels = globe.render(160, 120);
        assert_ne!(without_labels.as_raw(), kitty_labels.as_raw());

        globe.compact_destination_markers = true;
        let fallback_frame = globe.render(160, 120);
        globe.show_labels = false;
        let fallback_without_labels = globe.render(160, 120);
        assert_eq!(fallback_frame.as_raw(), fallback_without_labels.as_raw());
    }
}
