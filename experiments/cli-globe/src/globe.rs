//! Software 3D globe: equirectangular texture on a sphere, great-circle arcs, hop dots.
//! Renders into an RGB framebuffer — no WebGL, no HTML.

use image::{DynamicImage, Rgb, RgbImage};

#[derive(Clone, Debug)]
pub struct Hop {
    pub lat: f32,
    pub lon: f32,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct Path {
    pub app: String,
    pub host: String,
    pub color: [u8; 3],
    pub hops: Vec<Hop>,
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
}

impl GlobeRenderer {
    pub const ZOOM_MIN: f32 = 1.0;
    pub const ZOOM_MAX: f32 = 10.0;
    /// Halfblocks: tight on the map (low pixel density needs the close-up).
    pub const ZOOM_DEFAULT_HALF: f32 = 3.4;
    /// Braille: denser than halfblocks, so slightly more zoomed out.
    pub const ZOOM_DEFAULT_BRAILLE: f32 = 2.4;
    /// Kitty/pixel: more zoomed out so the whole globe reads clearly.
    pub const ZOOM_DEFAULT_KITTY: f32 = 1.25;
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
        for path in &paths {
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
                                    &mut out,
                                    px as i32,
                                    py as i32,
                                    sx as i32,
                                    sy as i32,
                                    path.color,
                                    stroke,
                                );
                            }
                        }
                        prev = Some((sx, sy, front));
                    } else {
                        prev = None;
                    }
                }
            }

            for (i, hop) in path.hops.iter().enumerate() {
                if let Some((sx, sy, _d, front)) =
                    self.project(hop.lat, hop.lon, cx, cy, r, cyaw, syaw, cpitch, spitch)
                {
                    if !front && i + 1 != path.hops.len() {
                        continue;
                    }
                    if sx < 0.0 || sy < 0.0 || sx >= w as f32 || sy >= h as f32 {
                        continue;
                    }
                    let dest = i + 1 == path.hops.len();
                    let rad = if dest { dest_r } else { hop_r };
                    fill_circle(&mut out, sx as i32, sy as i32, rad, path.color);
                    if dest {
                        draw_circle(&mut out, sx as i32, sy as i32, rad + 1, [220, 230, 255]);
                    }
                }
            }
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

/// Lift midtones, crush blacks a bit, push saturation — reads better at low res.
fn boost_contrast(rgb: [u8; 3]) -> [u8; 3] {
    let r = rgb[0] as f32 / 255.0;
    let g = rgb[1] as f32 / 255.0;
    let b = rgb[2] as f32 / 255.0;
    // S-curve contrast around mid-gray.
    let contrast = |x: f32| {
        let x = ((x - 0.5) * 1.55 + 0.5).clamp(0.0, 1.0);
        // Slight gamma lift so land isn't muddy after dark lighting.
        x.powf(0.92)
    };
    let mut r = contrast(r);
    let mut g = contrast(g);
    let mut b = contrast(b);
    // Saturation boost (mix away from luma).
    let y = 0.30 * r + 0.59 * g + 0.11 * b;
    let sat = 1.35;
    r = (y + (r - y) * sat).clamp(0.0, 1.0);
    g = (y + (g - y) * sat).clamp(0.0, 1.0);
    b = (y + (b - y) * sat).clamp(0.0, 1.0);
    // Nudge oceans cooler / land warmer via blue-vs-luma.
    if b > r && b > g && y < 0.45 {
        b = (b * 1.08).min(1.0);
        r *= 0.92;
        g *= 0.96;
    } else if y > 0.28 {
        r = (r * 1.06).min(1.0);
        g = (g * 1.04).min(1.0);
    }
    [
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
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

fn draw_line(img: &mut RgbImage, mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: [u8; 3], thick: i32) {
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
