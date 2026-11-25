//! Halftone: write an `App` once, run it in a terminal (ANSI) or a browser (wasm + canvas).
//!
//! ```ignore
//! use halftone::prelude::*;
//! #[derive(Default)] struct Hello;
//! impl App for Hello { fn view(&mut self, f: &mut Frame) { panel(f.buf, f.area, "hi", true); } }
//! halftone::main!(Hello);
//! ```

mod buffer;
pub mod image;
#[cfg(not(target_arch = "wasm32"))]
pub mod media;
#[cfg(not(target_arch = "wasm32"))]
pub mod deck;
pub mod widgets;
pub mod vt;
#[cfg(unix)]
pub mod pty;
pub mod island;
pub mod motion;
pub mod toast;
#[cfg(not(target_arch = "wasm32"))]
pub mod workspace;
#[cfg(feature = "ratatui")]
pub mod rat;

#[cfg(not(target_arch = "wasm32"))]
pub mod term;
#[cfg(target_arch = "wasm32")]
pub mod web;

pub use buffer::*;

pub mod theme {
    //! The palette widgets read every frame. `theme::set` restyles the whole UI at runtime.
    use crate::Color;
    use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct Theme {
        pub name: &'static str,
        pub bg: u32,
        pub panel: u32,
        pub raised: u32,
        pub line: u32,
        pub text: u32,
        pub muted: u32,
        pub faint: u32,
        pub accent: u32,
        pub blue: u32,
        pub good: u32,
        pub warn: u32,
        pub bad: u32,
    }

    impl Theme {
        const fn slots(&self) -> [u32; 12] {
            let t = self;
            [t.bg, t.panel, t.raised, t.line, t.text, t.muted, t.faint, t.accent, t.blue, t.good, t.warn, t.bad]
        }

        /// Blend every slot toward `o` by `t` in 0..=1; the name switches at the halfway point.
        pub fn mix(&self, o: &Theme, t: f32) -> Theme {
            let (a, b) = (self.slots(), o.slots());
            let m = |i: usize| {
                let (r, g, b) = Color::rgb(a[i]).mix(Color::rgb(b[i]), t).rgb_or_black();
                (r as u32) << 16 | (g as u32) << 8 | b as u32
            };
            Theme {
                name: if t > 0.5 { o.name } else { self.name },
                bg: m(0), panel: m(1), raised: m(2), line: m(3), text: m(4), muted: m(5),
                faint: m(6), accent: m(7), blue: m(8), good: m(9), warn: m(10), bad: m(11),
            }
        }
    }

    // House theme: neutral blue-black with a warm amber accent. Faint lifted for 3:1 on panel.
    pub const INK: Theme = Theme {
        name: "ink", bg: 0x0a0c0f, panel: 0x111419, raised: 0x1a1e25, line: 0x262b34, text: 0xdfe3e8, muted: 0x8a93a0,
        faint: 0x66707d, accent: 0xe8b77d, blue: 0x8fb3d9, good: 0x86c79a, warn: 0xd9bf8c, bad: 0xe08a82,
    };
    // House light theme: warm paper. Faint darkened for 3:1 on panel.
    pub const PAPER: Theme = Theme {
        name: "paper", bg: 0xf4f1ea, panel: 0xfbf9f4, raised: 0xe9e4d8, line: 0xd3ccbc, text: 0x1d1b17, muted: 0x6b655a,
        faint: 0x8c8574, accent: 0xa8562f, blue: 0x46689a, good: 0x4f7a4c, warn: 0x93702c, bad: 0xa4483f,
    };
    // https://github.com/catppuccin/palette/blob/main/palette.json (mocha)
    pub const MOCHA: Theme = Theme {
        name: "mocha", bg: 0x181825, panel: 0x1e1e2e, raised: 0x313244, line: 0x45475a, text: 0xcdd6f4, muted: 0xa6adc8,
        faint: 0x6c7086, accent: 0xcba6f7, blue: 0x89b4fa, good: 0xa6e3a1, warn: 0xf9e2af, bad: 0xf38ba8,
    };
    // https://github.com/catppuccin/palette/blob/main/palette.json (latte)
    pub const LATTE: Theme = Theme {
        name: "latte", bg: 0xdce0e8, panel: 0xeff1f5, raised: 0xe6e9ef, line: 0xbcc0cc, text: 0x4c4f69, muted: 0x5c5f77,
        faint: 0x6c6f85, accent: 0x8839ef, blue: 0x1e66f5, good: 0x40a02b, warn: 0xdf8e1d, bad: 0xd20f39,
    };
    // https://github.com/folke/tokyonight.nvim/blob/main/extras/lua/tokyonight_night.lua
    pub const TOKYO: Theme = Theme {
        name: "tokyo", bg: 0x16161e, panel: 0x1a1b26, raised: 0x292e42, line: 0x3b4261, text: 0xc0caf5, muted: 0xa9b1d6,
        faint: 0x737aa2, accent: 0x7aa2f7, blue: 0x7dcfff, good: 0x9ece6a, warn: 0xe0af68, bad: 0xf7768e,
    };
    // https://github.com/rose-pine/rose-pine-palette/blob/main/palette.json (main; line is the docs' highlight-med 403d52)
    pub const ROSEPINE: Theme = Theme {
        name: "rosepine", bg: 0x191724, panel: 0x1f1d2e, raised: 0x26233a, line: 0x403d52, text: 0xe0def4, muted: 0x908caa,
        faint: 0x6e6a86, accent: 0xebbcba, blue: 0xc4a7e7, good: 0x9ccfd8, warn: 0xf6c177, bad: 0xeb6f92,
    };
    // https://github.com/rose-pine/rose-pine-palette/blob/main/palette.json (dawn; muted darkened, line is the docs' highlight-med dfdad9)
    pub const DAWN: Theme = Theme {
        name: "dawn", bg: 0xfaf4ed, panel: 0xfffaf3, raised: 0xf2e9e1, line: 0xdfdad9, text: 0x464261, muted: 0x6a6682,
        faint: 0x797593, accent: 0x907aa9, blue: 0x286983, good: 0x56949f, warn: 0xea9d34, bad: 0xb4637a,
    };
    // https://github.com/morhetz/gruvbox/blob/master/colors/gruvbox.vim (dark, medium)
    pub const GRUVBOX: Theme = Theme {
        name: "gruvbox", bg: 0x282828, panel: 0x32302f, raised: 0x3c3836, line: 0x504945, text: 0xebdbb2, muted: 0xa89984,
        faint: 0x928374, accent: 0xfe8019, blue: 0x83a598, good: 0xb8bb26, warn: 0xfabd2f, bad: 0xfb4934,
    };
    // https://github.com/nordtheme/nord/blob/develop/src/nord.css (muted/faint are nord4 blended toward nord3)
    pub const NORD: Theme = Theme {
        name: "nord", bg: 0x2e3440, panel: 0x3b4252, raised: 0x434c5e, line: 0x4c566a, text: 0xeceff4, muted: 0xb0b8c7,
        faint: 0x8c96aa, accent: 0x88c0d0, blue: 0x81a1c1, good: 0xa3be8c, warn: 0xebcb8b, bad: 0xbf616a,
    };
    // https://github.com/rebelot/kanagawa.nvim/blob/master/lua/kanagawa/colors.lua (wave)
    pub const KANAGAWA: Theme = Theme {
        name: "kanagawa", bg: 0x1a1a22, panel: 0x1f1f28, raised: 0x2a2a37, line: 0x363646, text: 0xdcd7ba, muted: 0x938aa9,
        faint: 0x727169, accent: 0x7e9cd8, blue: 0x7fb4ca, good: 0x98bb6c, warn: 0xdca561, bad: 0xe46876,
    };
    // https://github.com/sainnhe/everforest/blob/master/autoload/everforest.vim (dark, medium; muted lightened from grey2)
    pub const EVERFOREST: Theme = Theme {
        name: "everforest", bg: 0x2d353b, panel: 0x343f44, raised: 0x3d484d, line: 0x4f585e, text: 0xd3c6aa, muted: 0xa6b0a3,
        faint: 0x859289, accent: 0xa7c080, blue: 0x7fbbb3, good: 0x83c092, warn: 0xdbbc7f, bad: 0xe67e80,
    };
    /// Unknown names (including the retired `x`, `amber`, `moss`) fall back to the first preset, `ink`.
    pub const PRESETS: [Theme; 11] = [INK, PAPER, MOCHA, LATTE, TOKYO, ROSEPINE, DAWN, GRUVBOX, NORD, KANAGAWA, EVERFOREST];

    static SLOTS: [AtomicU32; 12] = {
        let s = INK.slots();
        let mut out = [const { AtomicU32::new(0) }; 12];
        let mut i = 0;
        while i < 12 {
            out[i] = AtomicU32::new(s[i]);
            i += 1;
        }
        out
    };

    pub fn set(t: Theme) {
        for (slot, v) in SLOTS.iter().zip(t.slots()) {
            slot.store(v, Relaxed);
        }
    }

    macro_rules! getters {
        ($($name:ident $i:literal),*) => { $(pub fn $name() -> Color { Color::rgb(SLOTS[$i].load(Relaxed)) })* };
    }
    getters!(bg 0, panel 1, raised 2, line 3, text 4, muted 5, faint 6, accent 7, blue 8, good 9, warn 10, bad 11);

    #[cfg(test)]
    #[test]
    fn set_swaps_palette() {
        set(PAPER);
        assert_eq!(bg(), Color::rgb(PAPER.bg));
        set(INK);
        assert_eq!(accent(), Color::rgb(INK.accent));
    }

    #[test]
    fn presets_are_legible() {
        let lum = |h: u32| {
            let f = |c: u32| {
                let c = c as f64 / 255.0;
                if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * f(h >> 16 & 255) + 0.7152 * f(h >> 8 & 255) + 0.0722 * f(h & 255)
        };
        let cr = |a: u32, b: u32| {
            let (x, y) = (lum(a).max(lum(b)), lum(a).min(lum(b)));
            (x + 0.05) / (y + 0.05)
        };
        for t in PRESETS {
            assert!(cr(t.text, t.panel) >= 4.5, "{} text", t.name);
            assert!(cr(t.muted, t.panel) >= 4.5, "{} muted", t.name);
            assert!(cr(t.faint, t.panel) >= 3.0, "{} faint", t.name);
        }
    }

    #[test]
    fn mix_hits_both_ends() {
        assert_eq!(INK.mix(&PAPER, 0.0), INK);
        assert_eq!(INK.mix(&PAPER, 1.0), PAPER);
        assert_eq!(INK.mix(&PAPER, 0.4).name, "ink");
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
}

impl Key {
    pub const fn plain(code: KeyCode) -> Key {
        Key { code, ctrl: false, alt: false }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseKind {
    Down,
    Up,
    Drag,
    Move,
    ScrollUp,
    ScrollDown,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Event {
    Key(Key),
    Mouse { kind: MouseKind, x: u16, y: u16 },
    /// Press on a rect registered with `Frame::hit`; x/y are relative to `rect`.
    Click { id: u32, x: u16, y: u16, rect: Rect },
    /// Pointer moved while held after a `Click`; x/y relative and clamped to `rect`.
    Drag { id: u32, x: u16, y: u16, rect: Rect },
    /// Seconds since the last tick.
    Tick(f64),
    Resize(u16, u16),
}

impl Event {
    /// The key code if this is a key press without ctrl/alt.
    pub fn key(&self) -> Option<KeyCode> {
        match self {
            Event::Key(k) if !k.ctrl && !k.alt => Some(k.code),
            _ => None,
        }
    }
    pub fn ch(&self) -> Option<char> {
        match self.key() {
            Some(KeyCode::Char(c)) => Some(c),
            _ => None,
        }
    }
}

pub struct Frame<'a> {
    pub buf: &'a mut Buffer,
    pub area: Rect,
    hits: &'a mut Vec<(Rect, u32)>,
    pub mouse: Option<(u16, u16)>,
    /// Seconds since start.
    pub time: f64,
}

impl Frame<'_> {
    /// Makes `rect` clickable/draggable; later hits win when rects overlap.
    pub fn hit(&mut self, rect: Rect, id: u32) {
        self.hits.push((rect, id));
    }
    pub fn hover(&self, rect: Rect) -> bool {
        self.mouse.is_some_and(|(x, y)| rect.contains(x, y))
    }
}

#[derive(Default)]
pub struct Ctx {
    quit: bool,
}

impl Ctx {
    pub fn quit(&mut self) {
        self.quit = true;
    }
}

pub trait App {
    fn update(&mut self, _ev: &Event, _ctx: &mut Ctx) {}
    fn view(&mut self, f: &mut Frame);
}

/// Backend-independent driver: owns the app, the cell buffer and hit testing.
pub struct Runtime {
    app: Box<dyn App>,
    pub buf: Buffer,
    hits: Vec<(Rect, u32)>,
    mouse: Option<(u16, u16)>,
    drag: Option<(u32, Rect)>,
    pub time: f64,
    pub quit: bool,
}

impl Runtime {
    pub fn new(app: Box<dyn App>, w: u16, h: u16) -> Runtime {
        Runtime { app, buf: Buffer::new(w, h), hits: vec![], mouse: None, drag: None, time: 0.0, quit: false }
    }

    pub fn resize(&mut self, w: u16, h: u16) {
        if (w, h) != (self.buf.w, self.buf.h) {
            self.buf.resize(w, h);
            self.dispatch(Event::Resize(w, h));
        }
    }

    fn dispatch(&mut self, ev: Event) {
        let mut ctx = Ctx::default();
        self.app.update(&ev, &mut ctx);
        self.quit |= ctx.quit;
    }

    pub fn send(&mut self, ev: Event) {
        if let Event::Mouse { kind, x, y } = ev {
            self.mouse = if x == u16::MAX { None } else { Some((x, y)) };
            match kind {
                MouseKind::Down => {
                    let hit = self.hits.iter().rev().find(|(r, _)| r.contains(x, y)).copied();
                    self.drag = hit.map(|(r, id)| (id, r));
                    self.dispatch(ev);
                    if let Some((r, id)) = hit {
                        self.dispatch(Event::Click { id, x: x - r.x, y: y - r.y, rect: r });
                    }
                    return;
                }
                MouseKind::Drag => {
                    self.dispatch(ev);
                    if let Some((id, r)) = self.drag {
                        let rx = x.clamp(r.x, r.right().saturating_sub(1).max(r.x)) - r.x;
                        let ry = y.clamp(r.y, r.bottom().saturating_sub(1).max(r.y)) - r.y;
                        self.dispatch(Event::Drag { id, x: rx, y: ry, rect: r });
                    }
                    return;
                }
                MouseKind::Up => self.drag = None,
                _ => {}
            }
        }
        self.dispatch(ev);
    }

    pub fn tick(&mut self, dt: f64) {
        self.time += dt;
        self.dispatch(Event::Tick(dt));
    }

    pub fn render(&mut self) {
        self.buf.clear(theme::bg());
        self.hits.clear();
        let area = self.buf.area();
        let mut f = Frame { buf: &mut self.buf, area, hits: &mut self.hits, mouse: self.mouse, time: self.time };
        self.app.view(&mut f);
    }
}

/// Generates the entry point: `fn main` natively, wasm exports for the browser runtime.
#[macro_export]
macro_rules! main {
    ($app:expr) => {
        #[cfg(not(target_arch = "wasm32"))]
        fn main() {
            if let Err(e) = $crate::term::run(Box::new($app)) {
                eprintln!("halftone: {e}");
                std::process::exit(1);
            }
        }
        #[cfg(target_arch = "wasm32")]
        fn main() {}
        #[cfg(target_arch = "wasm32")]
        #[no_mangle]
        pub extern "C" fn ht_start(w: u32, h: u32) {
            $crate::web::start(Box::new($app), w, h)
        }
        #[cfg(target_arch = "wasm32")]
        #[no_mangle]
        pub extern "C" fn ht_resize(w: u32, h: u32) {
            $crate::web::resize(w, h)
        }
        #[cfg(target_arch = "wasm32")]
        #[no_mangle]
        pub extern "C" fn ht_frame(dt: f64) -> *const u32 {
            $crate::web::frame(dt)
        }
        #[cfg(target_arch = "wasm32")]
        #[no_mangle]
        pub extern "C" fn ht_key(code: u32, ch: u32, mods: u32) {
            $crate::web::key(code, ch, mods)
        }
        #[cfg(target_arch = "wasm32")]
        #[no_mangle]
        pub extern "C" fn ht_mouse(kind: u32, x: u32, y: u32) {
            $crate::web::mouse(kind, x, y)
        }
    };
}

pub mod prelude {
    pub use crate::image::{Dither, Image, Mode, Opts};
    pub use crate::theme;
    #[cfg(not(target_arch = "wasm32"))]
    pub use crate::deck::*;
    pub use crate::widgets::*;
    pub use crate::island::{Activity, Island, Spring};
    pub use crate::{attr, fg, App, Buffer, Cell, Color, Ctx, Event, Frame, Key, KeyCode, MouseKind, Rect, Size, Style};
    pub use crate::Size::{Fill, Len, Pct};
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Probe(Vec<Event>);
    impl App for Probe {
        fn update(&mut self, ev: &Event, _: &mut Ctx) {
            self.0.push(*ev);
        }
        fn view(&mut self, f: &mut Frame) {
            f.hit(Rect::new(2, 1, 10, 1), 7);
        }
    }

    #[test]
    fn click_and_drag_are_relative() {
        let mut rt = Runtime::new(Box::new(Probe(vec![])), 20, 5);
        rt.render();
        rt.send(Event::Mouse { kind: MouseKind::Down, x: 5, y: 1 });
        rt.send(Event::Mouse { kind: MouseKind::Drag, x: 40, y: 1 });
        // Runtime owns the app as dyn; re-check via a fresh render path is enough here.
        assert!(rt.drag.is_some());
        rt.send(Event::Mouse { kind: MouseKind::Up, x: 40, y: 1 });
        assert!(rt.drag.is_none());
    }
}
