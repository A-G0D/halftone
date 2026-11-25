//! Cells, colours, styles and rect layout. Everything else draws into a `Buffer`.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Color {
    /// Terminal default / "leave unchanged" inside a `Style`.
    #[default]
    Reset,
    Rgb(u8, u8, u8),
}

impl Color {
    pub const fn rgb(hex: u32) -> Color {
        Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
    }

    /// Linear blend toward `o` by `t` in 0..=1. Reset counts as black.
    pub fn mix(self, o: Color, t: f32) -> Color {
        let (a, b) = (self.rgb_or_black(), o.rgb_or_black());
        let t = t.clamp(0.0, 1.0);
        let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
        Color::Rgb(l(a.0, b.0), l(a.1, b.1), l(a.2, b.2))
    }

    pub fn rgb_or_black(self) -> (u8, u8, u8) {
        match self {
            Color::Reset => (0, 0, 0),
            Color::Rgb(r, g, b) => (r, g, b),
        }
    }
}

pub mod attr {
    pub const BOLD: u8 = 1;
    pub const DIM: u8 = 2;
    pub const ITALIC: u8 = 4;
    pub const UNDERLINE: u8 = 8;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub attrs: u8,
}

impl Style {
    pub const fn new() -> Style {
        Style { fg: Color::Reset, bg: Color::Reset, attrs: 0 }
    }
    pub const fn fg(mut self, c: Color) -> Style {
        self.fg = c;
        self
    }
    pub const fn bg(mut self, c: Color) -> Style {
        self.bg = c;
        self
    }
    pub const fn bold(mut self) -> Style {
        self.attrs |= attr::BOLD;
        self
    }
    pub const fn underline(mut self) -> Style {
        self.attrs |= attr::UNDERLINE;
        self
    }
}

pub const fn fg(c: Color) -> Style {
    Style::new().fg(c)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: u8,
}

impl Default for Cell {
    fn default() -> Cell {
        Cell { ch: ' ', fg: Color::Reset, bg: Color::Reset, attrs: 0 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    Len(u16),
    Pct(u16),
    /// Share of what's left after Len/Pct, by weight.
    Fill(u16),
}

impl Rect {
    pub const fn new(x: u16, y: u16, w: u16, h: u16) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> u16 {
        self.x + self.w
    }
    pub fn bottom(&self) -> u16 {
        self.y + self.h
    }
    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
    pub fn inset(&self, dx: u16, dy: u16) -> Rect {
        let dx = dx.min(self.w / 2);
        let dy = dy.min(self.h / 2);
        Rect::new(self.x + dx, self.y + dy, self.w - 2 * dx, self.h - 2 * dy)
    }
    /// One-row rect at offset `i`, empty if out of range.
    pub fn row(&self, i: u16) -> Rect {
        if i >= self.h {
            return Rect::new(self.x, self.bottom(), self.w, 0);
        }
        Rect::new(self.x, self.y + i, self.w, 1)
    }
    pub fn rows(&self, sizes: &[Size], gap: u16) -> Vec<Rect> {
        split(self.h, sizes, gap)
            .into_iter()
            .map(|(o, l)| Rect::new(self.x, self.y + o, self.w, l))
            .collect()
    }
    pub fn cols(&self, sizes: &[Size], gap: u16) -> Vec<Rect> {
        split(self.w, sizes, gap)
            .into_iter()
            .map(|(o, l)| Rect::new(self.x + o, self.y, l, self.h))
            .collect()
    }
}

/// Returns (offset, len) per size, clamped to `total`.
fn split(total: u16, sizes: &[Size], gap: u16) -> Vec<(u16, u16)> {
    let gaps = gap.saturating_mul(sizes.len().saturating_sub(1) as u16);
    let avail = total.saturating_sub(gaps) as u32;
    let mut lens: Vec<u32> = sizes
        .iter()
        .map(|s| match *s {
            Size::Len(n) => n as u32,
            Size::Pct(p) => avail * p as u32 / 100,
            Size::Fill(_) => 0,
        })
        .collect();
    let fixed: u32 = lens.iter().sum();
    let mut left = avail.saturating_sub(fixed);
    let mut wleft: u32 = sizes.iter().map(|s| if let Size::Fill(w) = s { *w as u32 } else { 0 }).sum();
    for (i, s) in sizes.iter().enumerate() {
        if let Size::Fill(w) = *s {
            let share = if wleft == 0 { 0 } else { left * w as u32 / wleft };
            lens[i] = share;
            left -= share;
            wleft -= w as u32;
        }
    }
    let mut out = Vec::with_capacity(sizes.len());
    let mut off = 0u32;
    for l in lens {
        let o = off.min(total as u32);
        let l = l.min(total as u32 - o);
        out.push((o as u16, l as u16));
        off = o + l + gap as u32;
    }
    out
}

#[derive(Clone, Debug, Default)]
pub struct Buffer {
    pub w: u16,
    pub h: u16,
    pub cells: Vec<Cell>,
    /// Pictures to show at real pixels where the terminal can (kitty graphics protocol), over
    /// cells that hold a fallback rendering. Cleared every frame with the cells.
    pub images: Vec<(Rect, std::sync::Arc<crate::image::Image>)>,
}

impl Buffer {
    pub fn new(w: u16, h: u16) -> Buffer {
        Buffer { w, h, cells: vec![Cell::default(); w as usize * h as usize], images: vec![] }
    }
    pub fn area(&self) -> Rect {
        Rect::new(0, 0, self.w, self.h)
    }
    pub fn resize(&mut self, w: u16, h: u16) {
        *self = Buffer::new(w, h);
    }
    pub fn clear(&mut self, bg: Color) {
        self.images.clear();
        for c in &mut self.cells {
            *c = Cell { bg, ..Cell::default() };
        }
    }
    pub fn cell(&mut self, x: u16, y: u16) -> Option<&mut Cell> {
        if x < self.w && y < self.h {
            Some(&mut self.cells[y as usize * self.w as usize + x as usize])
        } else {
            None
        }
    }
    pub fn get(&self, x: u16, y: u16) -> Cell {
        if x < self.w && y < self.h {
            self.cells[y as usize * self.w as usize + x as usize]
        } else {
            Cell::default()
        }
    }
    /// Writes one glyph; Reset colours in `s` keep what's already there.
    pub fn set(&mut self, x: u16, y: u16, ch: char, s: Style) {
        if let Some(c) = self.cell(x, y) {
            c.ch = ch;
            if s.fg != Color::Reset {
                c.fg = s.fg;
            }
            if s.bg != Color::Reset {
                c.bg = s.bg;
            }
            c.attrs = s.attrs;
        }
    }
    pub fn fill(&mut self, r: Rect, bg: Color) {
        for y in r.y..r.bottom().min(self.h) {
            for x in r.x..r.right().min(self.w) {
                if let Some(c) = self.cell(x, y) {
                    *c = Cell { bg, ..Cell::default() };
                }
            }
        }
    }
    /// Prints at most `max` cells and returns the x after the last glyph.
    // shortcut: one cell per char, wide (CJK/emoji) glyphs will misalign; add a width table when needed.
    pub fn print(&mut self, x: u16, y: u16, s: &str, st: Style, max: u16) -> u16 {
        let mut cx = x;
        for ch in s.chars() {
            if cx >= x.saturating_add(max) {
                break;
            }
            self.set(cx, y, ch, st);
            cx += 1;
        }
        cx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_fills_exactly() {
        let r = Rect::new(0, 0, 100, 10);
        let c = r.cols(&[Size::Len(20), Size::Fill(1), Size::Fill(2)], 1);
        assert_eq!(c[0].w, 20);
        assert_eq!(c[1].x, 21);
        assert_eq!(c[2].right(), 100);
        assert_eq!(c[1].w + c[2].w, 78);
    }

    #[test]
    fn split_clamps_when_too_small() {
        let c = Rect::new(0, 0, 10, 1).cols(&[Size::Len(8), Size::Len(8)], 2);
        assert_eq!(c[1].x, 10);
        assert_eq!(c[1].w, 0);
    }

    #[test]
    fn set_keeps_bg_on_reset() {
        let mut b = Buffer::new(2, 1);
        b.fill(b.area(), Color::rgb(0x112233));
        b.set(0, 0, 'x', fg(Color::rgb(0xffffff)));
        assert_eq!(b.get(0, 0).bg, Color::rgb(0x112233));
        assert_eq!(b.get(0, 0).ch, 'x');
    }
}
