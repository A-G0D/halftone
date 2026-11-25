//! Browser backend. `web/runtime.js` calls these through the `main!` exports and paints
//! the cell grid on a canvas: 4 u32 per cell, `[char, fg, bg, attrs]`, colours as `1<<24 | rgb`, 0 = Reset.

use crate::{App, Color, Event, Key, KeyCode, MouseKind, Runtime};
use std::cell::RefCell;

#[link(wasm_import_module = "env")]
extern "C" {
    fn ht_log(ptr: *const u8, len: usize);
}

thread_local! {
    static RT: RefCell<Option<Runtime>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

fn with(f: impl FnOnce(&mut Runtime)) {
    RT.with(|rt| {
        if let Some(rt) = rt.borrow_mut().as_mut() {
            f(rt)
        }
    })
}

pub fn start(app: Box<dyn App>, w: u32, h: u32) {
    std::panic::set_hook(Box::new(|p| {
        let s = p.to_string();
        unsafe { ht_log(s.as_ptr(), s.len()) }
    }));
    RT.with(|rt| *rt.borrow_mut() = Some(Runtime::new(app, w as u16, h as u16)));
}

pub fn resize(w: u32, h: u32) {
    with(|rt| rt.resize(w as u16, h as u16));
}

fn color(c: Color) -> u32 {
    match c {
        Color::Reset => 0,
        Color::Rgb(r, g, b) => 1 << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32,
    }
}

/// Ticks, renders and returns the packed grid; valid until the next call.
// shortcut: ctx.quit() is ignored in the browser, there is nowhere to exit to.
pub fn frame(dt: f64) -> *const u32 {
    OUT.with(|out| {
        let mut out = out.borrow_mut();
        out.clear();
        with(|rt| {
            rt.tick(dt);
            rt.render();
            for c in &rt.buf.cells {
                out.extend([c.ch as u32, color(c.fg), color(c.bg), c.attrs as u32]);
            }
        });
        out.as_ptr()
    })
}

pub fn key(code: u32, ch: u32, mods: u32) {
    use KeyCode::*;
    let code = match code {
        0 => match char::from_u32(ch) {
            Some(c) => Char(c),
            None => return,
        },
        1 => Enter,
        2 => Esc,
        3 => Backspace,
        4 => Tab,
        5 => BackTab,
        6 => Up,
        7 => Down,
        8 => Left,
        9 => Right,
        10 => Home,
        11 => End,
        12 => PageUp,
        13 => PageDown,
        14 => Delete,
        _ => return,
    };
    with(|rt| rt.send(Event::Key(Key { code, ctrl: mods & 1 != 0, alt: mods & 2 != 0 })));
}

/// `kind` follows `MouseKind` order; x = 65535 means the pointer left the canvas.
pub fn mouse(kind: u32, x: u32, y: u32) {
    use MouseKind::*;
    let Some(&kind) = [Down, Up, Drag, Move, ScrollUp, ScrollDown].get(kind as usize) else { return };
    with(|rt| rt.send(Event::Mouse { kind, x: x.min(65535) as u16, y: y.min(65535) as u16 }));
}
