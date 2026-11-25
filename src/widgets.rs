//! Immediate-mode widgets. Each draws into the buffer; interactive ones register hit rects.

use crate::image::bayer;
use crate::{fg, theme, Buffer, Color, Event, Frame, KeyCode, Rect, Style};

/// Rounded panel with a title. Returns the content rect.
pub fn panel(b: &mut Buffer, r: Rect, title: &str, focus: bool) -> Rect {
    if r.w < 2 || r.h < 2 {
        return Rect::new(r.x, r.y, 0, 0);
    }
    b.fill(r, theme::panel());
    let line = if focus { theme::line().mix(theme::accent(), 0.55) } else { theme::line() };
    let s = fg(line);
    let (x0, y0, x1, y1) = (r.x, r.y, r.right() - 1, r.bottom() - 1);
    for x in x0 + 1..x1 {
        b.set(x, y0, '─', s);
        b.set(x, y1, '─', s);
    }
    for y in y0 + 1..y1 {
        b.set(x0, y, '│', s);
        b.set(x1, y, '│', s);
    }
    b.set(x0, y0, '╭', s);
    b.set(x1, y0, '╮', s);
    b.set(x0, y1, '╰', s);
    b.set(x1, y1, '╯', s);
    if !title.is_empty() && r.w > 6 {
        let t = fg(if focus { theme::text() } else { theme::muted() }).bold();
        b.set(x0 + 1, y0, ' ', t);
        let end = b.print(x0 + 2, y0, title, t, r.w - 5);
        b.set(end, y0, ' ', t);
    }
    r.inset(2, 1)
}

/// Label over a bold value. Returns the x where the next stat can start.
pub fn stat(b: &mut Buffer, x: u16, y: u16, label: &str, value: &str, color: Color) -> u16 {
    let a = b.print(x, y, label, fg(theme::muted()), u16::MAX - x);
    let v = b.print(x, y + 1, value, fg(color).bold(), u16::MAX - x);
    a.max(v) + 3
}

const H_EIGHTHS: [char; 9] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
const V_EIGHTHS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Horizontal bar with 1/8-cell precision and a gradient from `from` to `to`.
pub fn gauge(b: &mut Buffer, r: Rect, ratio: f32, from: Color, to: Color) {
    let total = (ratio.clamp(0.0, 1.0) * r.w as f32 * 8.0).round() as i32;
    for i in 0..r.w {
        let n = (total - i as i32 * 8).clamp(0, 8) as usize;
        let c = from.mix(to, i as f32 / r.w.max(2) as f32 - 1.0 / r.w.max(2) as f32);
        for y in r.y..r.bottom() {
            b.set(r.x + i, y, H_EIGHTHS[n], Style::new().fg(c).bg(theme::raised()));
        }
    }
}

/// Bar sparkline, newest value on the right, `r.h` rows tall.
pub fn sparkline(b: &mut Buffer, r: Rect, data: &[f32], max: f32, color: Color) {
    let n = data.len().min(r.w as usize);
    let tail = &data[data.len() - n..];
    let x0 = r.right() - n as u16;
    for (i, v) in tail.iter().enumerate() {
        let eighths = ((v / max.max(1e-6)).clamp(0.0, 1.0) * r.h as f32 * 8.0).round() as i32;
        for row in 0..r.h {
            let n = (eighths - row as i32 * 8).clamp(0, 8) as usize;
            if n > 0 {
                b.set(x0 + i as u16, r.bottom() - 1 - row, V_EIGHTHS[n], fg(color));
            }
        }
    }
}

/// Braille dot canvas: 2×4 dots per cell, one colour per cell (last write wins).
pub struct Canvas {
    pub w: u16,
    pub h: u16,
    bits: Vec<u8>,
    col: Vec<Color>,
}

const BIT: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

impl Canvas {
    /// Canvas sized in cells; dot space is `w*2` × `h*4`.
    pub fn new(w: u16, h: u16) -> Canvas {
        let n = w as usize * h as usize;
        Canvas { w, h, bits: vec![0; n], col: vec![Color::Reset; n] }
    }
    pub fn dot(&mut self, x: i32, y: i32, c: Color) {
        if x < 0 || y < 0 || x >= self.w as i32 * 2 || y >= self.h as i32 * 4 {
            return;
        }
        let i = (y / 4) as usize * self.w as usize + (x / 2) as usize;
        self.bits[i] |= BIT[(x % 2) as usize][(y % 4) as usize];
        self.col[i] = c;
    }
    pub fn line(&mut self, (mut x0, mut y0): (i32, i32), (x1, y1): (i32, i32), c: Color) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let mut err = dx + dy;
        loop {
            self.dot(x0, y0, c);
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
    pub fn draw(&self, b: &mut Buffer, x: u16, y: u16) {
        for cy in 0..self.h {
            for cx in 0..self.w {
                let i = cy as usize * self.w as usize + cx as usize;
                if self.bits[i] != 0 {
                    let ch = char::from_u32(0x2800 + self.bits[i] as u32).unwrap_or(' ');
                    b.set(x + cx, y + cy, ch, fg(self.col[i]));
                }
            }
        }
    }
}

/// Braille line chart with an ordered-dither (halftone) fill that fades with depth.
pub fn chart(b: &mut Buffer, r: Rect, data: &[f32], max: f32, line: Color, fill: Color) {
    if r.w == 0 || r.h == 0 || data.len() < 2 {
        return;
    }
    let mut c = Canvas::new(r.w, r.h);
    let (dw, dh) = (r.w as i32 * 2, r.h as i32 * 4);
    let n = data.len();
    let ys: Vec<i32> = (0..dw)
        .map(|dx| {
            let t = dx as f32 / (dw - 1).max(1) as f32 * (n - 1) as f32;
            let (i, f) = (t.floor() as usize, t.fract());
            let v = data[i] + (data[(i + 1).min(n - 1)] - data[i]) * f;
            ((1.0 - (v / max.max(1e-6)).clamp(0.0, 1.0)) * (dh - 1) as f32).round() as i32
        })
        .collect();
    for (dx, &top) in ys.iter().enumerate() {
        for dy in top + 1..dh {
            let depth = (dy - top) as f32 / (dh - top) as f32;
            if bayer(dx as u32, dy as u32) + 0.5 < 0.55 * (1.0 - depth) {
                c.dot(dx as i32, dy, fill.mix(theme::panel(), depth * 0.8));
            }
        }
    }
    for dx in 1..dw as usize {
        c.line((dx as i32 - 1, ys[dx - 1]), (dx as i32, ys[dx]), line);
    }
    c.draw(b, r.x, r.y);
}

/// Inline tabs. Hit ids are `id_base + index`. Returns the x after the last tab.
pub fn tabs(f: &mut Frame, x: u16, y: u16, labels: &[&str], active: usize, id_base: u32) -> u16 {
    let mut cx = x;
    for (i, l) in labels.iter().enumerate() {
        let w = l.chars().count() as u16 + 2;
        let r = Rect::new(cx, y, w, 1);
        let st = if i == active {
            Style::new().fg(theme::text()).bg(theme::raised()).bold()
        } else if f.hover(r) {
            Style::new().fg(theme::text())
        } else {
            Style::new().fg(theme::muted())
        };
        if i == active {
            f.buf.fill(r, theme::raised());
        }
        f.buf.print(cx + 1, y, l, st, w);
        f.hit(r, id_base + i as u32);
        cx += w + 1;
    }
    cx
}

pub fn button(f: &mut Frame, r: Rect, label: &str, id: u32, on: bool) {
    let hover = f.hover(r);
    let (bg, fgc) = if on {
        (theme::accent(), theme::bg())
    } else if hover {
        (theme::line(), theme::text())
    } else {
        (theme::raised(), theme::muted())
    };
    f.buf.fill(r, bg);
    let len = label.chars().count() as u16;
    let x = r.x + r.w.saturating_sub(len) / 2;
    let st = Style::new().fg(fgc).bg(bg);
    f.buf.print(x, r.y + r.h / 2, label, if on { st.bold() } else { st }, r.w);
    f.hit(r, id);
}

/// Scrolling list. `row` draws one item into its rect; hit ids are `id_base + index`.
pub fn list<T>(
    f: &mut Frame,
    r: Rect,
    items: &[T],
    selected: usize,
    id_base: u32,
    mut row: impl FnMut(&mut Buffer, Rect, &T, bool),
) {
    if r.h == 0 {
        return;
    }
    let off = selected.saturating_sub(r.h as usize - 1);
    for (i, item) in items.iter().enumerate().skip(off).take(r.h as usize) {
        let rr = r.row((i - off) as u16);
        let sel = i == selected;
        if sel {
            f.buf.fill(rr, theme::raised());
            f.buf.set(rr.x, rr.y, '▎', fg(theme::accent()));
        } else if f.hover(rr) {
            f.buf.fill(rr, theme::panel().mix(theme::raised(), 0.6));
        }
        row(f.buf, Rect::new(rr.x + 2, rr.y, rr.w.saturating_sub(2), 1), item, sel);
        f.hit(rr, id_base + i as u32);
    }
}

/// One-row slider for `v` in 0..=1. Turn clicks/drags back into a value with `slider_value`.
pub fn slider(f: &mut Frame, r: Rect, v: f32, id: u32) {
    if r.w == 0 {
        return;
    }
    let k = (v.clamp(0.0, 1.0) * (r.w - 1) as f32).round() as u16;
    let hot = f.hover(r);
    for i in 0..r.w {
        let (ch, c) = match i.cmp(&k) {
            std::cmp::Ordering::Less => ('━', theme::accent()),
            std::cmp::Ordering::Equal => ('●', if hot { theme::accent() } else { theme::text() }),
            std::cmp::Ordering::Greater => ('─', theme::line()),
        };
        f.buf.set(r.x + i, r.y, ch, fg(c));
    }
    f.hit(r, id);
}

/// Maps a relative x from `Event::Click`/`Event::Drag` onto 0..=1.
pub fn slider_value(x: u16, r: Rect) -> f32 {
    x as f32 / (r.w.max(2) - 1) as f32
}

#[derive(Default, Clone, Debug)]
pub struct TextInput {
    pub value: String,
    /// Cursor position in chars.
    pub cursor: usize,
}

impl TextInput {
    fn byte(&self, ci: usize) -> usize {
        self.value.char_indices().nth(ci).map_or(self.value.len(), |(b, _)| b)
    }

    /// Applies an editing key. Returns true if the event was used.
    pub fn handle(&mut self, ev: &Event) -> bool {
        let n = self.value.chars().count();
        match ev.key() {
            Some(KeyCode::Char(c)) => {
                let b = self.byte(self.cursor);
                self.value.insert(b, c);
                self.cursor += 1;
            }
            Some(KeyCode::Backspace) if self.cursor > 0 => {
                self.cursor -= 1;
                let b = self.byte(self.cursor);
                self.value.remove(b);
            }
            Some(KeyCode::Delete) if self.cursor < n => {
                let b = self.byte(self.cursor);
                self.value.remove(b);
            }
            Some(KeyCode::Left) => self.cursor = self.cursor.saturating_sub(1),
            Some(KeyCode::Right) => self.cursor = (self.cursor + 1).min(n),
            Some(KeyCode::Home) => self.cursor = 0,
            Some(KeyCode::End) => self.cursor = n,
            Some(KeyCode::Backspace | KeyCode::Delete) => {}
            _ => return false,
        }
        true
    }

    pub fn view(&self, f: &mut Frame, r: Rect, placeholder: &str, focus: bool) {
        f.buf.fill(r, theme::raised());
        if r.w < 2 {
            return;
        }
        let w = r.w as usize - 1;
        if self.value.is_empty() && !focus {
            f.buf.print(r.x + 1, r.y, placeholder, fg(theme::faint()), r.w - 1);
            return;
        }
        let off = self.cursor.saturating_sub(w - 1);
        f.buf.print(r.x + 1, r.y, &self.value.chars().skip(off).collect::<String>(), fg(theme::text()), r.w - 1);
        if focus && (f.time * 2.0) as i64 % 2 == 0 {
            let cx = r.x + 1 + (self.cursor - off) as u16;
            let ch = self.value.chars().nth(self.cursor).unwrap_or(' ');
            f.buf.set(cx, r.y, ch, Style::new().fg(theme::bg()).bg(theme::accent()));
        }
    }
}

/// 3×5 pixel glyph (M and W are 5 wide, N is 4), one row per byte, low bits right-aligned.
/// Lowercase draws as uppercase.
fn glyph(c: char) -> [u8; 5] {
    match c.to_ascii_uppercase() {
        'A' => [2, 5, 7, 5, 5], 'B' => [6, 5, 6, 5, 6], 'C' => [3, 4, 4, 4, 3], 'D' => [6, 5, 5, 5, 6],
        'E' => [7, 4, 6, 4, 7], 'F' => [7, 4, 6, 4, 4], 'G' => [3, 4, 5, 5, 3], 'H' => [5, 5, 7, 5, 5],
        'I' => [7, 2, 2, 2, 7], 'J' => [1, 1, 1, 5, 2], 'K' => [5, 5, 6, 5, 5], 'L' => [4, 4, 4, 4, 7],
        'M' => [17, 27, 21, 17, 17], 'N' => [9, 13, 11, 9, 9], 'O' => [2, 5, 5, 5, 2], 'P' => [6, 5, 6, 4, 4],
        'Q' => [2, 5, 5, 6, 3], 'R' => [6, 5, 6, 5, 5], 'S' => [3, 4, 2, 1, 6], 'T' => [7, 2, 2, 2, 2],
        'U' => [5, 5, 5, 5, 7], 'V' => [5, 5, 5, 5, 2], 'W' => [17, 17, 21, 27, 17], 'X' => [5, 5, 2, 5, 5],
        'Y' => [5, 5, 2, 2, 2], 'Z' => [7, 1, 2, 4, 7],
        '0' => [7, 5, 5, 5, 7], '1' => [2, 6, 2, 2, 2], '2' => [7, 1, 7, 4, 7], '3' => [7, 1, 7, 1, 7],
        '4' => [5, 5, 7, 1, 1], '5' => [7, 4, 7, 1, 7], '6' => [7, 4, 7, 5, 7], '7' => [7, 1, 1, 1, 1],
        '8' => [7, 5, 7, 5, 7], '9' => [7, 5, 7, 1, 7],
        '.' => [0, 0, 0, 0, 2], ',' => [0, 0, 0, 2, 4], ':' => [0, 2, 0, 2, 0], '!' => [2, 2, 2, 0, 2],
        '\'' => [2, 2, 0, 0, 0], '-' => [0, 0, 7, 0, 0], '+' => [0, 2, 7, 2, 0], '%' => [5, 1, 2, 4, 5],
        '$' => [3, 6, 2, 3, 6], '&' => [2, 5, 2, 5, 3], '?' => [6, 1, 2, 0, 2], '/' => [1, 1, 2, 4, 4],
        _ => [0; 5],
    }
}

/// Pixel columns a glyph spans: letters and digits keep all three, punctuation is trimmed, space is two.
fn glyph_cols(c: char) -> std::ops::Range<u16> {
    if c == ' ' || c.is_ascii_alphanumeric() {
        return 0..match c.to_ascii_uppercase() { ' ' => 2, 'M' | 'W' => 5, 'N' => 4, _ => 3 };
    }
    let m = glyph(c).iter().fold(0, |a, r| a | r);
    let lo = (0..3).find(|i| m & (4 >> i) != 0).unwrap_or(0);
    let hi = (0..3).rev().find(|i| m & (4 >> i) != 0).map_or(1, |i| i + 1);
    lo..hi
}

/// Cells `s` takes in `big` type.
pub fn big_width(s: &str) -> u16 {
    s.chars().map(|c| glyph_cols(c).len() as u16 + 1).sum::<u16>().saturating_sub(1)
}

/// Text three rows tall in a 3×5 pixel face drawn with half blocks, on the existing background.
/// Rolls like `deck::marquee` when wider than `w`. Returns the x after it.
pub fn big(b: &mut Buffer, x: u16, y: u16, w: u16, s: &str, color: Color, time: f64) -> u16 {
    let full = big_width(s);
    let (text, off) = if full <= w {
        (s.to_string(), 0)
    } else {
        let t = format!("{s}   ·   ");
        let cyc = big_width(&t) + 1;
        let k = time.rem_euclid(cyc as f64 / 8.0 + 2.5);
        (format!("{t}{s}"), if k < 2.5 { 0 } else { ((k - 2.5) * 8.0) as u16 })
    };
    let st = fg(color);
    let mut px = 0u16;
    for c in text.chars() {
        let g = glyph(c);
        let gw = glyph_cols(c).end.max(3);
        for col in glyph_cols(c) {
            if let Some(cx) = px.checked_sub(off).filter(|v| *v < w) {
                for row in 0..3 {
                    let on = |p: usize| p < 5 && g[p] >> (gw - 1 - col) & 1 != 0;
                    let ch = match (on(row * 2), on(row * 2 + 1)) {
                        (true, true) => '█',
                        (true, false) => '▀',
                        (false, true) => '▄',
                        _ => continue,
                    };
                    b.set(x + cx, y + row as u16, ch, st);
                }
            }
            px += 1;
        }
        px += 1;
    }
    x + full.min(w)
}

/// Loading text with a light band sweeping across it; `time` in seconds.
pub fn shimmer(b: &mut Buffer, x: u16, y: u16, s: &str, time: f64, max: u16) {
    let w = (s.chars().count() as u16).min(max);
    let head = (time * 0.8).fract() as f32 * (w as f32 + 16.0) - 8.0;
    for (i, ch) in s.chars().take(w as usize).enumerate() {
        let glow = (1.0 - (i as f32 - head).abs() / 8.0).max(0.0);
        b.set(x + i as u16, y, ch, fg(theme::faint().mix(theme::text(), glow)));
    }
}

/// Key hint pills: `[(key, label)]`. Returns the x after the last hint.
pub fn keys(b: &mut Buffer, x: u16, y: u16, hints: &[(&str, &str)]) -> u16 {
    let mut cx = x;
    for (k, l) in hints {
        let st = Style::new().fg(theme::text()).bg(theme::raised());
        b.set(cx, y, ' ', st);
        cx = b.print(cx + 1, y, k, st, u16::MAX - cx - 1);
        b.set(cx, y, ' ', st);
        cx = b.print(cx + 2, y, l, fg(theme::muted()), u16::MAX - cx - 2) + 2;
    }
    cx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Key;

    #[test]
    fn big_type_widths_and_pixels() {
        // Digits are 3 wide, the point is trimmed to 1, one gap between glyphs.
        assert_eq!(big_width("1.5"), 3 + 1 + 1 + 1 + 3);
        let mut b = Buffer::new(12, 3);
        big(&mut b, 0, 0, 12, "T", Color::rgb(0xffffff), 0.0);
        // T: full top bar, then a stem down the middle.
        assert_eq!((b.get(0, 0).ch, b.get(1, 0).ch, b.get(1, 2).ch), ('▀', '█', '▀'));
    }

    #[test]
    fn text_input_edits_unicode() {
        let mut t = TextInput::default();
        for c in "héllo".chars() {
            t.handle(&Event::Key(Key::plain(KeyCode::Char(c))));
        }
        t.handle(&Event::Key(Key::plain(KeyCode::Left)));
        t.handle(&Event::Key(Key::plain(KeyCode::Backspace)));
        assert_eq!(t.value, "hélo");
        assert_eq!(t.cursor, 3);
    }

    #[test]
    fn gauge_full_and_empty() {
        let mut b = Buffer::new(4, 1);
        let a = b.area();
        gauge(&mut b, a, 1.0, theme::good(), theme::bad());
        assert!(b.cells.iter().all(|c| c.ch == '█'));
        gauge(&mut b, a, 0.0, theme::good(), theme::bad());
        assert!(b.cells.iter().all(|c| c.ch == ' '));
    }

    #[test]
    fn canvas_braille_bits() {
        let mut c = Canvas::new(1, 1);
        c.dot(0, 0, theme::text());
        c.dot(1, 3, theme::text());
        let mut b = Buffer::new(1, 1);
        c.draw(&mut b, 0, 0);
        assert_eq!(b.get(0, 0).ch, '\u{2881}');
    }
}
