//! xterm-compatible parser + grid. Pure (no I/O), so it also builds for wasm. Colours stay
//! symbolic (`VtColor`) until `draw`, which is what lets a hosted TUI take the halftone theme.

use crate::{attr, Buffer, Color, Key, KeyCode, Rect};
use std::collections::VecDeque;

const INVERSE: u8 = 16;
const SCROLLBACK: usize = 2000;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VtColor {
    #[default]
    Default,
    Idx(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct VCell {
    ch: char,
    fg: VtColor,
    bg: VtColor,
    attrs: u8,
}

#[derive(Clone, Copy, Default)]
struct Pen {
    fg: VtColor,
    bg: VtColor,
    attrs: u8,
}

type Row = Vec<VCell>;

#[derive(Clone, Copy, PartialEq)]
enum St {
    Ground,
    Esc,
    /// ESC followed by an intermediate (`(`, `)`, `#`...); the next byte ends it.
    EscInter(u8),
    Csi,
    Osc,
    /// DCS / APC / PM / SOS payload, skipped until ST.
    Str,
}

/// How draw maps terminal colours onto the host theme.
#[derive(Clone, Copy)]
pub struct Palette {
    pub fg: Color,
    pub bg: Color,
    pub ansi: [Color; 16],
    /// 0..=1: how far truecolor/256-colour cube values are pulled toward the nearest theme colour.
    pub retheme: f32,
    /// Whether the app paints for a dark ground; draw sets it from the app's own Vt.
    pub app_dark: bool,
}

impl Palette {
    pub fn from_theme() -> Palette {
        use crate::theme::*;
        let (t, g) = (text(), |c: Color| c.mix(text(), 0.25));
        let cyan = blue().mix(good(), 0.5);
        Palette {
            fg: t,
            bg: bg(),
            ansi: [
                bg().mix(t, 0.35), bad(), good(), warn(), blue(), accent(), cyan, muted(),
                faint(), g(bad()), g(good()), g(warn()), g(blue()), g(accent()), g(cyan), t,
            ],
            retheme: 0.85,
            app_dark: luma(bg()) < luma(t),
        }
    }

    fn idx(&self, i: u8) -> Color {
        const L: [u8; 6] = [0, 95, 135, 175, 215, 255];
        match i {
            0..=15 => self.ansi[i as usize],
            16..=231 => {
                let n = (i - 16) as usize;
                Color::Rgb(L[n / 36], L[n / 6 % 6], L[n % 6])
            }
            _ => {
                let v = 8 + 10 * (i - 232);
                Color::Rgb(v, v, v)
            }
        }
    }

    fn resolve(&self, c: VtColor, is_fg: bool) -> Color {
        match c {
            VtColor::Default => if is_fg { self.fg } else { self.bg },
            VtColor::Idx(i) if i < 16 => self.ansi[i as usize],
            VtColor::Idx(i) => self.pull(self.idx(i)),
            VtColor::Rgb(r, g, b) => self.pull(Color::Rgb(r, g, b)),
        }
    }

    /// Greys land on the theme's own bg→text ramp at the same lightness; colours take the theme
    /// colour nearest in hue. Works on whatever an app paints, so it survives the app's updates.
    fn pull(&self, c: Color) -> Color {
        if self.retheme <= 0.0 {
            return c;
        }
        let (r, g, b) = c.rgb_or_black();
        let (hi, lo) = (r.max(g).max(b) as f32, r.min(g).min(b) as f32);
        let target = if hi - lo < 28.0 {
            // The app's own ground maps to ours, so a dark app still reads on a light theme.
            let l = luma(c);
            self.bg.mix(self.fg, if self.app_dark { l } else { 1.0 - l })
        } else {
            let h = hue(r, g, b);
            let gap = |o: &Color| {
                let (r, g, b) = o.rgb_or_black();
                let d = (hue(r, g, b) - h).abs() % 360.0;
                (d.min(360.0 - d) * 100.0) as i32
            };
            *self.ansi[1..7].iter().min_by_key(|o| gap(o)).unwrap()
        };
        c.mix(target, self.retheme)
    }
}

fn theme_dark() -> bool {
    luma(crate::theme::bg()) < luma(crate::theme::text())
}

fn luma(c: Color) -> f32 {
    let (r, g, b) = c.rgb_or_black();
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
}

fn hue(r: u8, g: u8, b: u8) -> f32 {
    let (r, g, b) = (r as f32, g as f32, b as f32);
    let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
    let d = (hi - lo).max(1.0);
    let h = if hi == r { (g - b) / d % 6.0 } else if hi == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 };
    (h * 60.0 + 360.0) % 360.0
}

pub struct Vt {
    cols: u16,
    rows: u16,
    g: Vec<Row>,
    /// The inactive screen (alt while on main, main while on alt).
    other: Vec<Row>,
    in_alt: bool,
    sb: VecDeque<Row>,
    /// Lines scrolled back from the live view; 0 = live.
    pub scroll: usize,
    cx: u16,
    cy: u16,
    wrap: bool,
    pen: Pen,
    saved: (u16, u16, Pen),
    alt_saved: (u16, u16, Pen),
    top: u16,
    bot: u16,
    cursor_vis: bool,
    gfx: bool,
    pub title: String,
    /// The polarity the app last saw: the theme's when it started or last asked (OSC 10/11).
    pub dark: bool,
    pub app_cursor: bool,
    pub bracketed_paste: bool,
    pub mouse_press: bool,
    pub mouse_drag: bool,
    pub mouse_sgr: bool,
    replies: Vec<u8>,
    st: St,
    esc_in_str: bool,
    buf: Vec<u8>,
    utf: Vec<u8>,
}

fn blank(bg: VtColor) -> VCell {
    VCell { ch: ' ', fg: VtColor::Default, bg, attrs: 0 }
}

impl Vt {
    pub fn new(cols: u16, rows: u16) -> Vt {
        let (cols, rows) = (cols.max(1), rows.max(1));
        let grid = vec![vec![blank(VtColor::Default); cols as usize]; rows as usize];
        Vt {
            cols, rows, g: grid.clone(), other: grid, in_alt: false, sb: VecDeque::new(), scroll: 0,
            cx: 0, cy: 0, wrap: false, pen: Pen::default(), saved: (0, 0, Pen::default()),
            alt_saved: (0, 0, Pen::default()), top: 0, bot: rows - 1, cursor_vis: true, gfx: false,
            title: String::new(), dark: theme_dark(), app_cursor: false, bracketed_paste: false, mouse_press: false,
            mouse_drag: false, mouse_sgr: false, replies: Vec::new(), st: St::Ground,
            esc_in_str: false, buf: Vec::new(), utf: Vec::new(),
        }
    }

    /// Bytes the terminal owes the child (DA, cursor position, colour queries).
    pub fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.replies)
    }

    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    pub fn cursor(&self) -> (u16, u16, bool) {
        (self.cx, self.cy, self.cursor_vis)
    }

    /// Visible text of live-screen row `y`, for tests and logging.
    pub fn row_text(&self, y: u16) -> String {
        self.g.get(y as usize).map(|r| r.iter().map(|c| c.ch).collect()).unwrap_or_default()
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let (cols, rows) = (cols.max(1), rows.max(1));
        let b = blank(VtColor::Default);
        while self.g.len() > rows as usize {
            if self.cy > 0 {
                let r = self.g.remove(0);
                self.cy -= 1;
                if !self.in_alt {
                    self.push_sb(r);
                }
            } else {
                self.g.pop();
            }
        }
        self.g.resize(rows as usize, vec![b; cols as usize]);
        self.other.truncate(rows as usize);
        self.other.resize(rows as usize, vec![b; cols as usize]);
        for r in self.g.iter_mut().chain(self.other.iter_mut()) {
            r.resize(cols as usize, b); // shortcut: no reflow, rewrap on resize if long lines matter
        }
        (self.cols, self.rows) = (cols, rows);
        (self.top, self.bot) = (0, rows - 1);
        self.cx = self.cx.min(cols - 1);
        self.cy = self.cy.min(rows - 1);
        self.wrap = false;
    }

    fn push_sb(&mut self, r: Row) {
        self.sb.push_back(r);
        if self.sb.len() > SCROLLBACK {
            self.sb.pop_front();
        }
        if self.scroll > 0 {
            self.scroll = (self.scroll + 1).min(self.sb.len());
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.byte(b);
        }
    }

    fn byte(&mut self, b: u8) {
        if matches!(self.st, St::Osc | St::Str) {
            if self.esc_in_str {
                self.esc_in_str = false;
                if b == b'\\' {
                    return self.end_str();
                }
                self.end_str(); // ESC started a new sequence; fall through to handle this byte
                self.st = St::Esc;
            } else if b == 0x1b {
                self.esc_in_str = true;
                return;
            } else if b == 7 && self.st == St::Osc {
                return self.end_str();
            } else {
                if self.buf.len() < 4096 {
                    self.buf.push(b);
                }
                return;
            }
        }
        if b == 0x18 || b == 0x1a {
            self.st = St::Ground; // CAN / SUB abort a sequence
            return;
        }
        if b < 0x20 && self.st != St::Ground {
            // C0 inside an escape sequence executes immediately, except ESC which restarts it.
            if b != 0x1b {
                return self.c0(b);
            }
        }
        match self.st {
            St::Ground => self.ground(b),
            St::Esc => self.esc(b),
            St::EscInter(i) => {
                if i == b'(' {
                    self.gfx = b == b'0';
                }
                self.st = St::Ground;
            }
            St::Csi => {
                if (0x40..=0x7e).contains(&b) {
                    self.st = St::Ground;
                    self.csi(b);
                } else if self.buf.len() < 256 {
                    self.buf.push(b);
                }
            }
            St::Osc | St::Str => {}
        }
    }

    fn end_str(&mut self) {
        if self.st == St::Osc {
            self.osc();
        }
        self.st = St::Ground;
        self.buf.clear();
    }

    fn c0(&mut self, b: u8) {
        match b {
            8 => self.cx = self.cx.saturating_sub(1),
            9 => self.cx = ((self.cx / 8 + 1) * 8).min(self.cols - 1),
            10..=12 => self.lf(),
            13 => self.cx = 0,
            _ => {} // BEL and the rest: nothing to do
        }
        self.wrap = false;
    }

    fn ground(&mut self, b: u8) {
        match b {
            0x1b => {
                self.st = St::Esc;
                self.buf.clear();
            }
            0..=0x1f => self.c0(b),
            0x20..=0x7e => {
                self.utf.clear();
                let c = b as char;
                self.put(if self.gfx { dec_line(c) } else { c });
            }
            0x7f => {}
            _ => {
                self.utf.push(b);
                let need = match self.utf[0] {
                    0xc0..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf7 => 4,
                    _ => 0,
                };
                if need == 0 || (b & 0xc0 != 0x80 && self.utf.len() > 1) {
                    self.utf.clear(); // stray or truncated sequence
                } else if self.utf.len() == need {
                    if let Some(c) = std::str::from_utf8(&self.utf).ok().and_then(|s| s.chars().next()) {
                        // zero-width marks would otherwise take a cell
                        if !matches!(c, '\u{300}'..='\u{36f}' | '\u{200b}'..='\u{200f}' | '\u{fe00}'..='\u{fe0f}') {
                            self.put(c); // shortcut: wide chars take one cell, add a 2-cell path for CJK/emoji
                        }
                    }
                    self.utf.clear();
                }
            }
        }
    }

    fn esc(&mut self, b: u8) {
        self.st = St::Ground;
        match b {
            b'[' => {
                self.st = St::Csi;
                self.buf.clear();
            }
            b']' => {
                self.st = St::Osc;
                self.buf.clear();
            }
            b'P' | b'_' | b'^' | b'X' => {
                self.st = St::Str;
                self.buf.clear();
            }
            b'(' | b')' | b'*' | b'+' | b'#' | b'%' | b' ' => self.st = St::EscInter(b),
            b'7' => self.saved = (self.cx, self.cy, self.pen),
            b'8' => (self.cx, self.cy, self.pen) = self.saved,
            b'D' => self.lf(),
            b'E' => {
                self.cx = 0;
                self.lf();
            }
            b'M' => {
                if self.cy == self.top {
                    self.scroll_down(1);
                } else {
                    self.cy = self.cy.saturating_sub(1);
                }
            }
            b'c' => {
                let (c, r) = (self.cols, self.rows);
                *self = Vt::new(c, r);
            }
            _ => {}
        }
        self.wrap = false;
    }

    fn put(&mut self, ch: char) {
        if self.wrap {
            self.wrap = false;
            self.cx = 0;
            self.lf();
        }
        let p = self.pen;
        self.g[self.cy as usize][self.cx as usize] = VCell { ch, fg: p.fg, bg: p.bg, attrs: p.attrs };
        if self.cx + 1 < self.cols {
            self.cx += 1;
        } else {
            self.wrap = true;
        }
    }

    fn lf(&mut self) {
        if self.cy == self.bot {
            self.scroll_up(1);
        } else if self.cy + 1 < self.rows {
            self.cy += 1;
        }
    }

    fn blank_row(&self) -> Row {
        vec![blank(self.pen.bg); self.cols as usize]
    }

    fn scroll_up(&mut self, n: u16) {
        for _ in 0..n.min(self.bot - self.top + 1) {
            let r = self.g.remove(self.top as usize);
            self.g.insert(self.bot as usize, self.blank_row());
            if !self.in_alt && self.top == 0 {
                self.push_sb(r);
            }
        }
    }

    fn scroll_down(&mut self, n: u16) {
        for _ in 0..n.min(self.bot - self.top + 1) {
            self.g.remove(self.bot as usize);
            self.g.insert(self.top as usize, self.blank_row());
        }
    }

    /// Blanks `[x0, x1)` of row `y` with the current background.
    fn clear(&mut self, y: u16, x0: u16, x1: u16) {
        let b = blank(self.pen.bg);
        if let Some(r) = self.g.get_mut(y as usize) {
            for c in &mut r[x0 as usize..(x1.min(self.cols)) as usize] {
                *c = b;
            }
        }
    }

    fn osc(&mut self) {
        let s = String::from_utf8_lossy(&self.buf).into_owned();
        let (n, arg) = s.split_once(';').unwrap_or((&s, ""));
        match n {
            "0" | "2" => self.title = arg.to_string(),
            "10" | "11" if arg == "?" => {
                self.dark = theme_dark();
                let (r, g, b) = if n == "10" { crate::theme::text() } else { crate::theme::bg() }.rgb_or_black();
                let h = |v: u8| format!("{v:02x}{v:02x}");
                self.replies.extend(format!("\x1b]{n};rgb:{}/{}/{}\x1b\\", h(r), h(g), h(b)).bytes());
            }
            _ => {}
        }
    }

    fn csi(&mut self, fin: u8) {
        let raw = std::mem::take(&mut self.buf);
        let prefix = raw.first().copied().filter(|c| b"?<=>".contains(c));
        if raw.iter().any(|c| (0x20..=0x2f).contains(c)) {
            return; // intermediates ($p, !p, SP q...) carry nothing we act on
        }
        let body = if prefix.is_some() { &raw[1..] } else { &raw[..] };
        // ';' separates params, ':' sub-params (kept so SGR 4:3 and 38:2::r:g:b parse).
        let ps: Vec<Vec<u32>> = std::str::from_utf8(body)
            .unwrap_or("")
            .split(';')
            .map(|g| g.split(':').map(|n| n.parse().unwrap_or(0)).collect())
            .collect();
        let p = |i: usize, d: u32| ps.get(i).map(|g| g[0]).filter(|&v| v != 0).unwrap_or(d);
        let (cols, rows) = (self.cols, self.rows);
        let n = p(0, 1).min(9999) as u16;
        self.wrap = false;
        match (prefix, fin) {
            (None, b'A') => {
                let lo = if self.cy >= self.top { self.top } else { 0 };
                self.cy = self.cy.saturating_sub(n).max(lo);
            }
            (None, b'B' | b'e') => {
                let hi = if self.cy <= self.bot { self.bot } else { rows - 1 };
                self.cy = (self.cy + n).min(hi);
            }
            (None, b'C' | b'a') => self.cx = (self.cx + n).min(cols - 1),
            (None, b'D') => self.cx = self.cx.saturating_sub(n),
            (None, b'E') => {
                self.cx = 0;
                self.cy = (self.cy + n).min(rows - 1);
            }
            (None, b'F') => {
                self.cx = 0;
                self.cy = self.cy.saturating_sub(n);
            }
            (None, b'G' | b'`') => self.cx = (n - 1).min(cols - 1),
            (None, b'd') => self.cy = (n - 1).min(rows - 1),
            (None, b'H' | b'f') => {
                self.cy = (p(0, 1) as u16 - 1).min(rows - 1);
                self.cx = (p(1, 1) as u16 - 1).min(cols - 1);
            }
            (None, b'J') => match p(0, 0) {
                0 => {
                    self.clear(self.cy, self.cx, cols);
                    (self.cy + 1..rows).for_each(|y| self.clear(y, 0, cols));
                }
                1 => {
                    (0..self.cy).for_each(|y| self.clear(y, 0, cols));
                    self.clear(self.cy, 0, self.cx + 1);
                }
                m => {
                    (0..rows).for_each(|y| self.clear(y, 0, cols));
                    if m == 3 {
                        self.sb.clear();
                        self.scroll = 0;
                    }
                }
            },
            (None, b'K') => match p(0, 0) {
                0 => self.clear(self.cy, self.cx, cols),
                1 => self.clear(self.cy, 0, self.cx + 1),
                _ => self.clear(self.cy, 0, cols),
            },
            (None, b'X') => self.clear(self.cy, self.cx, self.cx + n),
            (None, b'@') => {
                let n = n.min(cols - self.cx) as usize;
                let b = blank(self.pen.bg);
                let r = &mut self.g[self.cy as usize];
                r.truncate(cols as usize - n);
                (0..n).for_each(|_| r.insert(self.cx as usize, b));
            }
            (None, b'P') => {
                let n = n.min(cols - self.cx) as usize;
                let b = blank(self.pen.bg);
                let r = &mut self.g[self.cy as usize];
                r.drain(self.cx as usize..self.cx as usize + n);
                r.extend(std::iter::repeat(b).take(n));
            }
            (None, b'L' | b'M') if (self.top..=self.bot).contains(&self.cy) => {
                for _ in 0..n.min(self.bot - self.cy + 1) {
                    if fin == b'L' {
                        self.g.remove(self.bot as usize);
                        self.g.insert(self.cy as usize, self.blank_row());
                    } else {
                        self.g.remove(self.cy as usize);
                        self.g.insert(self.bot as usize, self.blank_row());
                    }
                }
                self.cx = 0;
            }
            (None, b'S') => self.scroll_up(n),
            (None, b'T') => self.scroll_down(n),
            (None, b'r') => {
                let (t, b) = (p(0, 1) as u16 - 1, (p(1, rows as u32) as u16).min(rows) - 1);
                if t < b {
                    (self.top, self.bot, self.cx, self.cy) = (t, b, 0, 0);
                }
            }
            (None, b's') => self.saved = (self.cx, self.cy, self.pen),
            (None, b'u') => (self.cx, self.cy, self.pen) = self.saved,
            (None, b'm') => self.sgr(&ps),
            (None, b'c') if p(0, 0) == 0 => self.replies.extend(b"\x1b[?62;22c"),
            (None, b'n') => match p(0, 0) {
                5 => self.replies.extend(b"\x1b[0n"),
                6 => self.replies.extend(format!("\x1b[{};{}R", self.cy + 1, self.cx + 1).bytes()),
                _ => {}
            },
            (Some(b'?'), b'u') => self.replies.extend(b"\x1b[?0u"),
            (Some(b'?'), b'h' | b'l') => {
                let on = fin == b'h';
                for g in &ps {
                    match g[0] {
                        1 => self.app_cursor = on,
                        25 => self.cursor_vis = on,
                        47 | 1047 | 1049 => self.alt(on, g[0] == 1049),
                        1000 => self.mouse_press = on,
                        1002 => self.mouse_drag = on,
                        1006 => self.mouse_sgr = on,
                        2004 => self.bracketed_paste = on,
                        _ => {} // 2026 sync and the rest are ignored
                    }
                }
            }
            _ => {}
        }
    }

    fn alt(&mut self, on: bool, save_cursor: bool) {
        if on == self.in_alt {
            return;
        }
        if on && save_cursor {
            self.alt_saved = (self.cx, self.cy, self.pen);
        }
        std::mem::swap(&mut self.g, &mut self.other);
        self.in_alt = on;
        if on {
            let b = blank(VtColor::Default);
            self.g.iter_mut().for_each(|r| r.fill(b));
            self.scroll = 0;
        } else if save_cursor {
            (self.cx, self.cy, self.pen) = self.alt_saved;
        }
        (self.top, self.bot) = (0, self.rows - 1);
    }

    fn sgr(&mut self, ps: &[Vec<u32>]) {
        let mut i = 0;
        while i < ps.len() {
            let g = &ps[i];
            let pen = &mut self.pen;
            match g[0] {
                0 => *pen = Pen::default(),
                1 => pen.attrs |= attr::BOLD,
                2 => pen.attrs |= attr::DIM,
                3 => pen.attrs |= attr::ITALIC,
                4 => pen.attrs = if g.get(1) == Some(&0) { pen.attrs & !attr::UNDERLINE } else { pen.attrs | attr::UNDERLINE },
                7 => pen.attrs |= INVERSE,
                22 => pen.attrs &= !(attr::BOLD | attr::DIM),
                23 => pen.attrs &= !attr::ITALIC,
                24 => pen.attrs &= !attr::UNDERLINE,
                27 => pen.attrs &= !INVERSE,
                n @ 30..=37 => pen.fg = VtColor::Idx(n as u8 - 30),
                n @ 90..=97 => pen.fg = VtColor::Idx(n as u8 - 90 + 8),
                n @ 40..=47 => pen.bg = VtColor::Idx(n as u8 - 40),
                n @ 100..=107 => pen.bg = VtColor::Idx(n as u8 - 100 + 8),
                39 => pen.fg = VtColor::Default,
                49 => pen.bg = VtColor::Default,
                n @ (38 | 48) => {
                    // colon form is one group; semicolon form spreads over the next groups
                    let flat: Vec<u32> = if g.len() > 1 {
                        g.clone()
                    } else {
                        let take = if ps.get(i + 1).map(|x| x[0]) == Some(5) { 3 } else { 5 };
                        let v: Vec<u32> = ps[i..].iter().take(take).map(|x| x[0]).collect();
                        i += v.len() - 1;
                        v
                    };
                    let c = match flat.get(1) {
                        Some(5) => flat.get(2).map(|&v| VtColor::Idx(v as u8)),
                        Some(2) if flat.len() >= 5 => {
                            let t = &flat[flat.len() - 3..];
                            Some(VtColor::Rgb(t[0] as u8, t[1] as u8, t[2] as u8))
                        }
                        _ => None,
                    };
                    if let Some(c) = c {
                        if n == 38 { self.pen.fg = c } else { self.pen.bg = c }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// Paints the (scrolled) screen into `r`, resolving colours through `pal`.
    pub fn draw(&self, b: &mut Buffer, r: Rect, pal: &Palette) {
        let total = self.sb.len() + self.g.len();
        let first = total - self.g.len() - self.scroll.min(self.sb.len());
        let (cx, cy, vis) = self.cursor();
        let pal = &Palette { app_dark: self.dark, ..*pal };
        for y in 0..r.h.min(self.rows) {
            let li = first + y as usize;
            let row = if li < self.sb.len() { &self.sb[li] } else { &self.g[li - self.sb.len()] };
            for x in 0..r.w.min(self.cols) {
                let c = row.get(x as usize).copied().unwrap_or(blank(VtColor::Default));
                let (mut f, mut g) = (pal.resolve(c.fg, true), pal.resolve(c.bg, false));
                if (c.attrs & INVERSE != 0) != (vis && self.scroll == 0 && (x, y) == (cx, cy)) {
                    std::mem::swap(&mut f, &mut g);
                }
                if let Some(d) = b.cell(r.x + x, r.y + y) {
                    *d = crate::Cell { ch: c.ch, fg: f, bg: g, attrs: c.attrs & 0x0f };
                }
            }
        }
    }
}

/// DEC special graphics (ESC ( 0) to Unicode box drawing.
fn dec_line(c: char) -> char {
    match c {
        'j' => '┘', 'k' => '┐', 'l' => '┌', 'm' => '└', 'n' => '┼', 'q' => '─', 't' => '├',
        'u' => '┤', 'v' => '┴', 'w' => '┬', 'x' => '│', 'a' => '▒', '`' => '◆', 'f' => '°',
        'g' => '±', '~' => '·', _ => c,
    }
}

/// Encodes a halftone key press as the bytes an xterm would send.
pub fn key_bytes(k: &Key, app_cursor: bool) -> Vec<u8> {
    let m = 1 + if k.alt { 2 } else { 0 } + if k.ctrl { 4 } else { 0 };
    let tilde = |n: u8| if m > 1 { format!("\x1b[{n};{m}~") } else { format!("\x1b[{n}~") };
    let cur = |c: char| match (m > 1, app_cursor) {
        (true, _) => format!("\x1b[1;{m}{c}"),
        (_, true) => format!("\x1bO{c}"),
        _ => format!("\x1b[{c}"),
    };
    let s = match k.code {
        KeyCode::Char(c) => {
            let mut v = Vec::new();
            if k.alt {
                v.push(0x1b);
            }
            match c {
                'a'..='z' | 'A'..='Z' if k.ctrl => v.push(c.to_ascii_lowercase() as u8 - b'a' + 1),
                ' ' | '@' if k.ctrl => v.push(0),
                '[' if k.ctrl => v.push(0x1b),
                '\\' if k.ctrl => v.push(0x1c),
                ']' if k.ctrl => v.push(0x1d),
                _ => v.extend(c.to_string().bytes()),
            }
            return v;
        }
        KeyCode::Enter => "\r".into(),
        KeyCode::Tab => "\t".into(),
        KeyCode::BackTab => "\x1b[Z".into(),
        KeyCode::Backspace => "\x7f".into(),
        KeyCode::Esc => "\x1b".into(),
        KeyCode::Up => cur('A'),
        KeyCode::Down => cur('B'),
        KeyCode::Right => cur('C'),
        KeyCode::Left => cur('D'),
        KeyCode::Home => cur('H'),
        KeyCode::End => cur('F'),
        KeyCode::PageUp => tilde(5),
        KeyCode::PageDown => tilde(6),
        KeyCode::Delete => tilde(3),
    };
    let mut v = Vec::new();
    // alt on a modifier-less sequence (Enter, Tab, Backspace, Esc) is an ESC prefix
    if k.alt && !s.starts_with("\x1b[") && !s.starts_with("\x1bO") && s != "\x1b" {
        v.push(0x1b);
    }
    v.extend(s.bytes());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vt(s: &str) -> Vt {
        let mut v = Vt::new(10, 4);
        v.feed(s.as_bytes());
        v
    }

    #[test]
    fn sgr_truecolor() {
        let v = vt("\x1b[1;38;2;1;2;3;48;5;200mA\x1b[38:2::9:8:7mB\x1b[0mC");
        assert_eq!(v.g[0][0].fg, VtColor::Rgb(1, 2, 3));
        assert_eq!(v.g[0][0].bg, VtColor::Idx(200));
        assert_eq!(v.g[0][0].attrs, attr::BOLD);
        assert_eq!(v.g[0][1].fg, VtColor::Rgb(9, 8, 7));
        assert_eq!(v.g[0][2].fg, VtColor::Default);
        assert_eq!(vt("\x1b[91;42mX").g[0][0].fg, VtColor::Idx(9));
    }

    #[test]
    fn cursor_and_erase() {
        let mut v = vt("hello\r\nworld\x1b[2;3H\x1b[K\x1b[A!");
        assert_eq!(v.row_text(0), "he!lo     ");
        assert_eq!(v.row_text(1), "wo        ");
        v.feed(b"\x1b[2J\x1b[3;4Hx\x1b[1K");
        assert_eq!(v.row_text(0).trim(), "");
        assert_eq!(v.row_text(2).trim(), "");
        assert_eq!(vt("abcdef\x1b[1;3H\x1b[2P").row_text(0), "abef      ");
        assert_eq!(vt("abcdef\x1b[1;3H\x1b[2@").row_text(0), "ab  cdef  ");
    }

    #[test]
    fn alt_screen_restores() {
        let mut v = vt("main\x1b[?1049hALT\x1b[2J");
        assert_eq!(v.row_text(0).trim(), "");
        v.feed(b"\x1b[?1049l");
        assert_eq!(v.row_text(0).trim(), "main");
        assert_eq!(v.cursor(), (4, 0, true));
    }

    #[test]
    fn scroll_region_and_scrollback() {
        let v = vt("1\r\n2\r\n3\r\n4\x1b[2;3r\x1b[3;1H\n\n");
        assert_eq!((v.row_text(0).trim(), v.row_text(1).trim(), v.row_text(3).trim()), ("1", "", "4"));
        assert_eq!(v.sb.len(), 0);
        let v = vt("1\r\n2\r\n3\r\n4\r\n5");
        assert_eq!(v.sb.len(), 1);
        assert_eq!(v.row_text(3), "5         ");
    }

    #[test]
    fn replies() {
        let mut v = vt("\x1b[c\x1b[2;3H\x1b[6n\x1b[5n\x1b[?u\x1b]11;?\x07");
        let r = String::from_utf8(v.take_replies()).unwrap();
        assert!(r.starts_with("\x1b[?62;22c\x1b[2;3R\x1b[0n\x1b[?0u\x1b]11;rgb:"), "{r:?}");
        assert!(v.take_replies().is_empty());
    }

    #[test]
    fn osc_title_utf8_and_dcs() {
        let v = vt("\x1bPq payload\x1b\\\x1b]0;hi\x1b\\é─");
        assert_eq!(v.title, "hi");
        assert_eq!(v.row_text(0).chars().take(3).collect::<String>(), "é─ ");
    }

    #[test]
    fn modes_and_draw() {
        let mut v = vt("\x1b[?1h\x1b[?2004h\x1b[?1006h\x1b[?25l\x1b[38;2;250;0;0mR");
        assert!(v.app_cursor && v.bracketed_paste && v.mouse_sgr && !v.cursor().2);
        v.resize(5, 2);
        let mut b = Buffer::new(8, 4);
        let mut pal = Palette::from_theme();
        pal.retheme = 0.0;
        v.draw(&mut b, Rect::new(1, 1, 5, 2), &pal);
        assert_eq!(b.get(1, 1).ch, 'R');
        assert_eq!(b.get(1, 1).fg, Color::Rgb(250, 0, 0));
        pal.retheme = 1.0;
        v.draw(&mut b, Rect::new(1, 1, 5, 2), &pal);
        assert_eq!(b.get(1, 1).fg, pal.ansi[1]);
        // A grey sits on the theme's bg→text ramp, never on a hue.
        let g = pal.pull(Color::Rgb(128, 128, 128));
        let l = luma(Color::Rgb(128, 128, 128));
        assert_eq!(g, pal.bg.mix(pal.fg, if pal.app_dark { l } else { 1.0 - l }));
        // For an app painting the other polarity, black means the opposite end of our ramp.
        let flipped = Palette { app_dark: !pal.app_dark, retheme: 1.0, ..pal };
        assert_eq!(flipped.pull(Color::Rgb(0, 0, 0)), if pal.app_dark { pal.fg } else { pal.bg });
    }

    #[test]
    fn keys() {
        let k = |code, ctrl, alt| key_bytes(&Key { code, ctrl, alt }, false);
        assert_eq!(k(KeyCode::Char('c'), true, false), [3]);
        assert_eq!(k(KeyCode::Char('x'), false, true), b"\x1bx");
        assert_eq!(k(KeyCode::Up, false, false), b"\x1b[A");
        assert_eq!(key_bytes(&Key::plain(KeyCode::Up), true), b"\x1bOA");
        assert_eq!(k(KeyCode::Right, true, false), b"\x1b[1;5C");
        assert_eq!(k(KeyCode::Delete, false, false), b"\x1b[3~");
        assert_eq!(k(KeyCode::Enter, false, false), b"\r");
    }
}
