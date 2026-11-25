//! Image decoding (BMP, PPM) and rendering into cells: half blocks, braille, shade blocks, ASCII.

use crate::{Buffer, Color, Rect, Style};

#[derive(Clone, Debug)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub px: Vec<[u8; 3]>,
}

const MAX_PIXELS: u64 = 1 << 26;

fn u16le(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}
fn u32le(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

impl Image {
    /// Decodes an uncompressed BMP (24/32 bpp) or binary PPM (P6).
    pub fn decode(b: &[u8]) -> Result<Image, String> {
        match b.get(..2) {
            Some(b"BM") => Image::bmp(b),
            Some(b"P6") => Image::ppm(b),
            _ => Err("unsupported image: expected BMP or PPM (P6)".into()),
        }
    }

    fn bmp(b: &[u8]) -> Result<Image, String> {
        let bad = || "truncated or malformed BMP".to_string();
        let off = u32le(b, 10).ok_or_else(bad)? as usize;
        let w = u32le(b, 18).ok_or_else(bad)? as i32;
        let h = u32le(b, 22).ok_or_else(bad)? as i32;
        let bpp = u16le(b, 28).ok_or_else(bad)?;
        let comp = u32le(b, 30).ok_or_else(bad)?;
        // shortcut: BITFIELDS is assumed to be the usual BGRA mask layout; read the masks if odd files show up.
        if !(bpp == 24 || bpp == 32) || !(comp == 0 || comp == 3) {
            return Err(format!("BMP: only 24/32-bit uncompressed is supported (got {bpp} bpp, compression {comp})"));
        }
        let (w, top_down, h) = (w.unsigned_abs(), h < 0, h.unsigned_abs());
        if w == 0 || h == 0 || w as u64 * h as u64 > MAX_PIXELS {
            return Err(format!("BMP: bad size {w}x{h}"));
        }
        let bp = bpp as usize / 8;
        let stride = (w as usize * bp + 3) & !3;
        if b.len() < off + stride * h as usize {
            return Err(bad());
        }
        let mut px = Vec::with_capacity(w as usize * h as usize);
        for y in 0..h as usize {
            let sy = if top_down { y } else { h as usize - 1 - y };
            let row = &b[off + sy * stride..];
            for x in 0..w as usize {
                let p = &row[x * bp..];
                px.push([p[2], p[1], p[0]]);
            }
        }
        Ok(Image { w, h, px })
    }

    fn ppm(b: &[u8]) -> Result<Image, String> {
        let mut i = 2;
        let mut num = || -> Result<u32, String> {
            loop {
                match b.get(i) {
                    Some(c) if c.is_ascii_whitespace() => i += 1,
                    Some(b'#') => {
                        while b.get(i).is_some_and(|&c| c != b'\n') {
                            i += 1;
                        }
                    }
                    _ => break,
                }
            }
            let s = i;
            while b.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            std::str::from_utf8(&b[s..i]).ok().and_then(|t| t.parse().ok()).ok_or("PPM: bad header".to_string())
        };
        let (w, h, max) = (num()?, num()?, num()?);
        // shortcut: 8-bit samples only; 16-bit PPM (max > 255) is rejected.
        if max == 0 || max > 255 || w == 0 || h == 0 || w as u64 * h as u64 > MAX_PIXELS {
            return Err(format!("PPM: unsupported {w}x{h} max {max}"));
        }
        let data = b.get(i + 1..i + 1 + w as usize * h as usize * 3).ok_or("PPM: truncated")?;
        let s = |v: u8| (v as u32 * 255 / max) as u8;
        let px = data.chunks_exact(3).map(|p| [s(p[0]), s(p[1]), s(p[2])]).collect();
        Ok(Image { w, h, px })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    /// ▀ with fg = top pixel, bg = bottom: two colour pixels per cell.
    #[default]
    Half,
    /// Quadrant glyphs (▘▝▖▗▚▞▌▐…): 2×2 samples per cell split into the best two colours, so
    /// edges get twice the horizontal detail of half blocks with no visible stair-step grid.
    Quad,
    /// 2×4 dots per cell, one colour per cell.
    Braille,
    /// Shade blocks " ░▒▓█".
    Blocks,
    Ascii,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Dither {
    None,
    #[default]
    Bayer,
    Floyd,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Opts {
    pub mode: Mode,
    pub dither: Dither,
    /// 1.0 = unchanged.
    pub contrast: f32,
}

impl Default for Opts {
    fn default() -> Opts {
        Opts { mode: Mode::Half, dither: Dither::Bayer, contrast: 1.0 }
    }
}

const BAYER: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

/// 4×4 ordered-dither threshold in -0.5..0.5.
pub fn bayer(x: u32, y: u32) -> f32 {
    (BAYER[(y % 4 * 4 + x % 4) as usize] as f32 + 0.5) / 16.0 - 0.5
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Box-filters `img` onto a `gw`×`gh` grid, with contrast, as 0..1 floats.
fn sample(img: &Image, gw: u32, gh: u32, contrast: f32) -> Vec<[f32; 3]> {
    let mut out = Vec::with_capacity((gw * gh) as usize);
    for gy in 0..gh {
        let (y0, y1) = (gy * img.h / gh, ((gy + 1) * img.h / gh).max(gy * img.h / gh + 1).min(img.h));
        for gx in 0..gw {
            let (x0, x1) = (gx * img.w / gw, ((gx + 1) * img.w / gw).max(gx * img.w / gw + 1).min(img.w));
            let mut acc = [0u32; 3];
            for y in y0..y1 {
                for p in &img.px[(y * img.w + x0) as usize..(y * img.w + x1) as usize] {
                    for k in 0..3 {
                        acc[k] += p[k] as u32;
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)).max(1) as f32 * 255.0;
            out.push(acc.map(|a| ((a as f32 / n - 0.5) * contrast + 0.5).clamp(0.0, 1.0)));
        }
    }
    out
}

/// Quantizes luma `l` (w×h, 0..1) to `levels` steps.
fn quantize(mut l: Vec<f32>, w: usize, h: usize, levels: usize, d: Dither) -> Vec<usize> {
    let top = (levels - 1) as f32;
    let mut out = vec![0; l.len()];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let v = match d {
                Dither::Bayer => l[i] + bayer(x as u32, y as u32) / top,
                _ => l[i],
            };
            let q = (v * top).round().clamp(0.0, top);
            out[i] = q as usize;
            if d == Dither::Floyd {
                let e = l[i] - q / top;
                let mut push = |dx: isize, dy: usize, f: f32| {
                    let nx = x as isize + dx;
                    if nx >= 0 && (nx as usize) < w && y + dy < h {
                        l[(y + dy) * w + nx as usize] += e * f;
                    }
                };
                push(1, 0, 7.0 / 16.0);
                push(-1, 1, 3.0 / 16.0);
                push(0, 1, 5.0 / 16.0);
                push(1, 1, 1.0 / 16.0);
            }
        }
    }
    out
}

/// Best two-colour split of a 2×2 block: (mask of the fg pixels, fg mean, bg mean).
fn quad_split(p: &[[f32; 3]; 4]) -> (usize, [f32; 3], [f32; 3]) {
    let mean = |m: usize, on: bool| {
        let (mut acc, mut n) = ([0f32; 3], 0f32);
        for (i, c) in p.iter().enumerate() {
            if (m >> i & 1 == 1) == on {
                (acc[0], acc[1], acc[2], n) = (acc[0] + c[0], acc[1] + c[1], acc[2] + c[2], n + 1.0);
            }
        }
        if n == 0.0 { acc } else { [acc[0] / n, acc[1] / n, acc[2] / n] }
    };
    let mut best = (15, mean(15, true), mean(15, true), f32::MAX);
    // Masks 1..=7 cover every split once (the rest are the same splits with colours swapped).
    for m in [15usize, 1, 2, 3, 4, 5, 6, 7] {
        let (f, k) = (mean(m, true), mean(m, false));
        let err: f32 = p.iter().enumerate().map(|(i, c)| {
            let t = if m >> i & 1 == 1 { f } else { k };
            (0..3).map(|j| (c[j] - t[j]).powi(2)).sum::<f32>()
        }).sum();
        if err < best.3 {
            best = (m, f, k, err);
        }
    }
    (best.0, best.1, best.2)
}

fn color(c: [f32; 3]) -> Color {
    let b = |v: f32| (v * 255.0).round() as u8;
    Color::Rgb(b(c[0]), b(c[1]), b(c[2]))
}

const SHADES: [char; 5] = [' ', '░', '▒', '▓', '█'];
const ASCII: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

/// Draws `img` centred in `r`, keeping square pixels (cells are ~1:2). Returns the rect used.
/// Set by the terminal backend when the terminal speaks the kitty graphics protocol: `draw`
/// then also hands the picture over at full resolution.
pub static PIXELS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn draw(b: &mut Buffer, r: Rect, img: &Image, o: Opts) -> Rect {
    let out = draw_cells(b, r, img, o);
    if PIXELS.load(std::sync::atomic::Ordering::Relaxed) && out.w > 0 {
        // The picture sits above cell backgrounds and below text: blank the glyphs it replaces.
        for y in out.y..out.bottom() {
            for x in out.x..out.right() {
                if let Some(c) = b.cell(x, y) {
                    c.ch = ' ';
                }
            }
        }
        b.images.push((out, std::sync::Arc::new(img.clone())));
    }
    out
}

fn draw_cells(b: &mut Buffer, r: Rect, img: &Image, o: Opts) -> Rect {
    if r.w == 0 || r.h == 0 || img.w == 0 || img.h == 0 {
        return Rect::new(r.x, r.y, 0, 0);
    }
    // Sub-pixels per cell; both grids give square sub-pixels on a 1:2 cell. Quad lays out like
    // half blocks, then samples twice as wide.
    let (sx, sy) = if o.mode == Mode::Braille { (2, 4) } else { (1, 2) };
    let scale = (r.w as f32 * sx as f32 / img.w as f32).min(r.h as f32 * sy as f32 / img.h as f32);
    let cw = ((img.w as f32 * scale / sx as f32).round() as u16).clamp(1, r.w);
    let ch = ((img.h as f32 * scale / sy as f32).round() as u16).clamp(1, r.h);
    let out = Rect::new(r.x + (r.w - cw) / 2, r.y + (r.h - ch) / 2, cw, ch);
    let (gw, gh) = (cw as u32 * if o.mode == Mode::Quad { 2 } else { sx }, ch as u32 * sy);
    let g = sample(img, gw, gh, o.contrast);
    let at = |x: u32, y: u32| g[(y * gw + x) as usize];

    match o.mode {
        Mode::Half => {
            for cy in 0..ch as u32 {
                for cx in 0..cw as u32 {
                    let st = Style::new().fg(color(at(cx, cy * 2))).bg(color(at(cx, cy * 2 + 1)));
                    b.set(out.x + cx as u16, out.y + cy as u16, '▀', st);
                }
            }
        }
        Mode::Quad => {
            const GLYPH: [char; 16] = [' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█'];
            for cy in 0..ch as u32 {
                for cx in 0..cw as u32 {
                    let p = [at(cx * 2, cy * 2), at(cx * 2 + 1, cy * 2), at(cx * 2, cy * 2 + 1), at(cx * 2 + 1, cy * 2 + 1)];
                    let (m, f, k) = quad_split(&p);
                    let st = Style::new().fg(color(f)).bg(color(k));
                    b.set(out.x + cx as u16, out.y + cy as u16, GLYPH[m], st);
                }
            }
        }
        Mode::Braille => {
            const BIT: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
            let q = quantize(g.iter().map(|&c| luma(c)).collect(), gw as usize, gh as usize, 2, o.dither);
            for cy in 0..ch as u32 {
                for cx in 0..cw as u32 {
                    let (mut bits, mut acc, mut n) = (0u8, [0f32; 3], 0f32);
                    for dy in 0..4 {
                        for dx in 0..2 {
                            let (x, y) = (cx * 2 + dx, cy * 4 + dy);
                            if q[(y * gw + x) as usize] == 1 {
                                bits |= BIT[dx as usize][dy as usize];
                                let c = at(x, y);
                                (0..3).for_each(|k| acc[k] += c[k]);
                                n += 1.0;
                            }
                        }
                    }
                    if bits != 0 {
                        let ch = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
                        b.set(out.x + cx as u16, out.y + cy as u16, ch, Style::new().fg(color(acc.map(|a| a / n))));
                    }
                }
            }
        }
        Mode::Blocks | Mode::Ascii => {
            let ramp: &[char] = if o.mode == Mode::Blocks { &SHADES } else { &ASCII };
            let cells: Vec<[f32; 3]> = (0..ch as u32)
                .flat_map(|cy| (0..cw as u32).map(move |cx| (cx, cy)))
                .map(|(cx, cy)| {
                    let (a, c) = (at(cx, cy * 2), at(cx, cy * 2 + 1));
                    [(a[0] + c[0]) / 2.0, (a[1] + c[1]) / 2.0, (a[2] + c[2]) / 2.0]
                })
                .collect();
            let q = quantize(cells.iter().map(|&c| luma(c)).collect(), cw as usize, ch as usize, ramp.len(), o.dither);
            for (i, c) in cells.iter().enumerate() {
                let (x, y) = (i as u16 % cw, i as u16 / cw);
                // Brighten the glyph colour so thin ASCII strokes still read as the pixel's hue.
                let lit = c.map(|v| v.max(0.0).powf(0.7));
                b.set(out.x + x, out.y + y, ramp[q[i]], Style::new().fg(color(lit)));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quad_split_finds_the_edge() {
        let (w, k) = ([1.0, 1.0, 1.0], [0.0, 0.0, 0.0]);
        assert_eq!(quad_split(&[w, w, w, w]).0, 15);
        let (m, f, b) = quad_split(&[w, k, w, k]);
        assert_eq!(m, 5);
        assert_eq!((f, b), (w, k));
    }

    fn bmp(w: i32, h: i32, px: &[[u8; 3]]) -> Vec<u8> {
        let stride = ((w * 3 + 3) & !3) as usize;
        let mut v = b"BM".to_vec();
        v.resize(54, 0);
        v[10] = 54;
        v[14] = 40;
        v[18..22].copy_from_slice(&w.to_le_bytes());
        v[22..26].copy_from_slice(&h.to_le_bytes());
        v[28] = 24;
        for row in px.chunks(w as usize) {
            let mut r: Vec<u8> = row.iter().flat_map(|p| [p[2], p[1], p[0]]).collect();
            r.resize(stride, 0);
            v.extend(r);
        }
        v
    }

    #[test]
    fn bmp_bottom_up_and_top_down() {
        let rows = [[255, 0, 0], [0, 0, 255]];
        let up = Image::decode(&bmp(1, 2, &rows)).unwrap();
        assert_eq!(up.px, vec![[0, 0, 255], [255, 0, 0]]);
        let down = Image::decode(&bmp(1, -2, &rows)).unwrap();
        assert_eq!(down.px, vec![[255, 0, 0], [0, 0, 255]]);
    }

    #[test]
    fn ppm_with_comment() {
        let mut v = b"P6\n# hi\n2 1\n255\n".to_vec();
        v.extend([1, 2, 3, 4, 5, 6]);
        let img = Image::decode(&v).unwrap();
        assert_eq!((img.w, img.h), (2, 1));
        assert_eq!(img.px[1], [4, 5, 6]);
    }

    #[test]
    fn rejects_truncated() {
        assert!(Image::decode(&bmp(4, 4, &[[0; 3]; 16])[..60]).is_err());
        assert!(Image::decode(b"P6 2 2 255\n\0").is_err());
    }

    #[test]
    fn half_mode_fills_rect_with_upper_halves() {
        let img = Image { w: 4, h: 4, px: vec![[200, 100, 50]; 16] };
        let mut b = Buffer::new(10, 10);
        let a = b.area();
        let r = draw(&mut b, a, &img, Opts::default());
        assert_eq!((r.w, r.h), (10, 5));
        assert_eq!(b.get(r.x, r.y).ch, '▀');
        assert_eq!(b.get(r.x, r.y).fg, Color::Rgb(200, 100, 50));
    }

    #[test]
    fn floyd_preserves_mean() {
        let q = quantize(vec![0.25; 64], 8, 8, 2, Dither::Floyd);
        let lit = q.iter().filter(|&&v| v == 1).count();
        assert!((12..=20).contains(&lit), "{lit}");
    }
}
