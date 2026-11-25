//! Native backend: raw mode via `stty`, input parsing, diffed ANSI output. No crates.

use crate::{attr, App, Buffer, Cell, Color, Event, Key, KeyCode, MouseKind, Rect, Runtime};
use std::ffi::c_ulong;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

extern "C" {
    fn ioctl(fd: i32, req: c_ulong, ...) -> i32;
    fn isatty(fd: i32) -> i32;
    fn signal(sig: i32, handler: extern "C" fn(i32)) -> usize;
}

/// Set by SIGTERM or SIGHUP, so a killed or closed session still puts the terminal back.
static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn on_stop(_: i32) {
    STOP.store(true, Relaxed);
}

#[cfg(target_os = "macos")]
const TIOCGWINSZ: c_ulong = 0x4008_7468;
#[cfg(not(target_os = "macos"))]
const TIOCGWINSZ: c_ulong = 0x5413;

// Alt screen, hide cursor, button+any-motion mouse in SGR encoding.
const ENTER: &str = "\x1b[?1049h\x1b[?25l\x1b[?1002h\x1b[?1003h\x1b[?1006h\x1b[2J";
const LEAVE: &str = "\x1b_Ga=d,d=A,q=2\x1b\\\x1b[?1006l\x1b[?1003l\x1b[?1002l\x1b[0m\x1b[?25h\x1b[?1049l";
static KEEP_CTRL_C: AtomicBool = AtomicBool::new(false);

/// Hands Ctrl+C to the app as a key instead of quitting, for apps that host other programs.
pub fn keep_ctrl_c(on: bool) {
    KEEP_CTRL_C.store(on, Relaxed);
}

/// Kitty graphics: Kitty, Ghostty and WezTerm say so in their environment. `HALFTONE_GRAPHICS=cells`
/// turns it off, `=kitty` forces it on (e.g. through ssh).
fn kitty_graphics() -> bool {
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    match env("HALFTONE_GRAPHICS").as_str() {
        "kitty" => return true,
        "cells" => return false,
        _ => {}
    }
    if !env("TMUX").is_empty() {
        return false;
    }
    env("TERM") == "xterm-kitty" || env("TERM") == "xterm-ghostty" || !env("KITTY_WINDOW_ID").is_empty() || matches!(env("TERM_PROGRAM").as_str(), "ghostty" | "WezTerm")
}

fn base64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for (i, sh) in [18, 12, 6, 0].iter().enumerate() {
            o.push(if i <= c.len() { A[(n >> sh & 63) as usize] as char } else { '=' });
        }
    }
    o
}

/// Cheap fingerprint so a still cover is sent once, not every frame.
fn fingerprint(img: &crate::image::Image) -> u64 {
    let mut h: u64 = (img.w as u64) << 32 | img.h as u64;
    for p in img.px.iter().step_by(61) {
        h = h.wrapping_mul(1_099_511_628_211) ^ ((p[0] as u64) << 16 | (p[1] as u64) << 8 | p[2] as u64);
    }
    h
}

/// Places each of the buffer's pictures (ids 1..), resending only those that changed, and
/// deletes the ones that went away.
fn kitty(s: &mut String, buf: &Buffer, shown: &mut Vec<(Rect, u64)>) {
    for (i, (r, img)) in buf.images.iter().enumerate() {
        let id = i + 1;
        let fp = fingerprint(img);
        if shown.get(i) == Some(&(*r, fp)) {
            continue;
        }
        let raw: Vec<u8> = img.px.iter().flatten().copied().collect();
        let data = base64(&raw);
        let _ = write!(s, "\x1b[{};{}H", r.y + 1, r.x + 1);
        let chunks: Vec<&str> = data.as_bytes().chunks(4096).map(|c| std::str::from_utf8(c).unwrap_or("")).collect();
        for (k, c) in chunks.iter().enumerate() {
            let more = if k + 1 < chunks.len() { 1 } else { 0 };
            if k == 0 {
                let _ = write!(s, "\x1b_Ga=T,f=24,s={},v={},i={id},p=1,c={},r={},z=-1,C=1,q=2,m={more};{c}\x1b\\", img.w, img.h, r.w, r.h);
            } else {
                let _ = write!(s, "\x1b_Gm={more};{c}\x1b\\");
            }
        }
        if i < shown.len() {
            shown[i] = (*r, fp);
        } else {
            shown.push((*r, fp));
        }
    }
    for id in buf.images.len() + 1..=shown.len() {
        let _ = write!(s, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\");
    }
    shown.truncate(buf.images.len());
}

/// Frame interval from `HALFTONE_FPS` (positive number, e.g. `12`), else 33 ms (30 fps).
fn frame() -> Duration {
    static FRAME: OnceLock<Duration> = OnceLock::new();
    *FRAME.get_or_init(|| {
        std::env::var("HALFTONE_FPS")
            .ok()
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|f| f.is_finite() && *f > 0.0)
            .and_then(|f| Duration::try_from_secs_f64(1.0 / f).ok())
            .unwrap_or(Duration::from_millis(33))
    })
}

#[repr(C)]
#[derive(Default)]
struct Winsize {
    row: u16,
    col: u16,
    xpx: u16,
    ypx: u16,
}

pub fn size() -> (u16, u16) {
    let mut ws = Winsize::default();
    // SAFETY: TIOCGWINSZ writes exactly one `winsize` through the pointer.
    let ok = unsafe { ioctl(1, TIOCGWINSZ, &mut ws as *mut Winsize) } == 0;
    if ok && ws.col > 0 && ws.row > 0 {
        (ws.col, ws.row)
    } else {
        (80, 24)
    }
}

fn stty(args: &[&str]) -> Result<String, String> {
    let o = Command::new("stty")
        .args(args)
        .stdin(Stdio::inherit())
        .output()
        .map_err(|e| format!("stty: {e}"))?;
    if !o.status.success() {
        return Err(format!("stty {}: {}", args.join(" "), String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn restore(saved: &str) {
    let mut out = std::io::stdout();
    let _ = out.write_all(LEAVE.as_bytes());
    let _ = out.flush();
    let _ = stty(&[saved]);
}

/// Runs `app` full-screen until it calls `ctx.quit()` or the user presses Ctrl+C.
pub fn run(app: Box<dyn App>) -> Result<(), String> {
    if std::env::var_os("HALFTONE_AGENT").is_some() {
        return agent(app);
    }
    // SAFETY: isatty only inspects the fd.
    if unsafe { isatty(0) == 0 || isatty(1) == 0 } {
        return Err("needs an interactive terminal (stdin and stdout must be a tty)".into());
    }
    let saved = stty(&["-g"])?;
    stty(&["raw", "-echo"])?;
    // SAFETY: the handler only stores to an atomic. 1 is SIGHUP and 15 SIGTERM on macOS and Linux.
    unsafe {
        signal(1, on_stop);
        signal(15, on_stop);
    }
    let hook_saved = saved.clone();
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore(&hook_saved);
        prev(info);
    }));
    let res = event_loop(app);
    restore(&saved);
    res
}

fn event_loop(app: Box<dyn App>) -> Result<(), String> {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 4096];
        while let Ok(n @ 1..) = stdin.read(&mut buf) {
            if tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });

    let tc = truecolor();
    let pixels = kitty_graphics();
    crate::image::PIXELS.store(pixels, Relaxed);
    let mut shown: Vec<(Rect, u64)> = vec![];
    let mut out = std::io::stdout().lock();
    out.write_all(ENTER.as_bytes()).map_err(|e| e.to_string())?;
    let (w, h) = size();
    let mut rt = Runtime::new(app, w, h);
    let mut prev: Option<Buffer> = None;
    let mut last = Instant::now();
    let mut s = String::new();

    while !rt.quit && !STOP.load(Relaxed) {
        let (w, h) = size();
        if (w, h) != (rt.buf.w, rt.buf.h) {
            rt.resize(w, h);
            prev = None;
        }
        let now = Instant::now();
        rt.tick(now.duration_since(last).as_secs_f64() * crate::media::speed());
        last = now;
        rt.render();

        s.clear();
        s.push_str("\x1b[?2026h");
        diff(&mut s, prev.as_ref(), &rt.buf, tc);
        if pixels {
            kitty(&mut s, &rt.buf, &mut shown);
        }
        s.push_str("\x1b[?2026l");
        out.write_all(s.as_bytes()).and_then(|_| out.flush()).map_err(|e| e.to_string())?;
        prev = Some(rt.buf.clone());

        // Input wakes the loop early so clicks and keys repaint immediately.
        match rx.recv_timeout(frame().saturating_sub(now.elapsed())) {
            Ok(bytes) => {
                for ev in parse(&bytes) {
                    if matches!(ev, Event::Key(Key { code: KeyCode::Char('c'), ctrl: true, .. })) && !KEEP_CTRL_C.load(Relaxed) {
                        rt.quit = true;
                    }
                    rt.send(ev);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => rt.quit = true,
        }
    }
    Ok(())
}

/// One line of the agent protocol.
#[derive(Debug, PartialEq)]
enum Cmd {
    Send(Vec<Event>),
    Tick(f64),
    Resize(u16, u16),
    Snap,
    Ansi,
    Quit,
}

fn command(line: &str) -> Option<Cmd> {
    let mut it = line.split_whitespace();
    let op = it.next()?;
    let mut num = || it.next()?.parse::<u16>().ok();
    let mouse = |kind, x, y| Event::Mouse { kind, x, y };
    Some(match op {
        "key" => {
            let name = line.split_whitespace().nth(1)?;
            let (ctrl, name) = name.strip_prefix("ctrl+").map_or((false, name), |n| (true, n));
            let code = match name {
                "enter" => KeyCode::Enter,
                "esc" => KeyCode::Esc,
                "tab" => KeyCode::Tab,
                "backtab" => KeyCode::BackTab,
                "backspace" => KeyCode::Backspace,
                "up" => KeyCode::Up,
                "down" => KeyCode::Down,
                "left" => KeyCode::Left,
                "right" => KeyCode::Right,
                "home" => KeyCode::Home,
                "end" => KeyCode::End,
                "pgup" => KeyCode::PageUp,
                "pgdn" => KeyCode::PageDown,
                "delete" => KeyCode::Delete,
                "space" => KeyCode::Char(' '),
                s if s.chars().count() == 1 => KeyCode::Char(s.chars().next()?),
                _ => return None,
            };
            Cmd::Send(vec![Event::Key(Key { code, ctrl, alt: false })])
        }
        "click" => {
            let (x, y) = (num()?, num()?);
            Cmd::Send(vec![mouse(MouseKind::Down, x, y), mouse(MouseKind::Up, x, y)])
        }
        "drag" => Cmd::Send(vec![mouse(MouseKind::Drag, num()?, num()?)]),
        "move" => Cmd::Send(vec![mouse(MouseKind::Move, num()?, num()?)]),
        "tick" => Cmd::Tick(line.split_whitespace().nth(1).map_or(Some(1.0 / 30.0), |s| s.parse().ok())?),
        "resize" => Cmd::Resize(num()?, num()?),
        "snap" => Cmd::Snap,
        "ansi" => Cmd::Ansi,
        "quit" => Cmd::Quit,
        _ => return None,
    })
}

/// Headless mode for scripts and AI agents: commands on stdin, screen snapshots on stdout.
/// `HALFTONE_AGENT=1`, optional `HALFTONE_SIZE=WxH` (default 100x30). Each snapshot ends with `---`.
fn agent(app: Box<dyn App>) -> Result<(), String> {
    let (w, h) = std::env::var("HALFTONE_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((100, 30));
    let mut rt = Runtime::new(app, w, h);
    rt.tick(0.0);
    rt.render();
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Some(cmd) = command(&line) else {
            writeln!(out, "? {line}").map_err(|e| e.to_string())?;
            continue;
        };
        match cmd {
            Cmd::Send(evs) => evs.into_iter().for_each(|e| rt.send(e)),
            Cmd::Tick(secs) => {
                let steps = (secs * 30.0).round().max(1.0) as u32;
                for _ in 0..steps {
                    rt.tick(secs / steps as f64);
                }
            }
            Cmd::Resize(w, h) => rt.resize(w, h),
            Cmd::Snap | Cmd::Ansi => {
                rt.render();
                let s = if cmd == Cmd::Ansi {
                    to_ansi(&rt.buf, true)
                } else {
                    let mut s = String::new();
                    for y in 0..rt.buf.h {
                        let row: String = (0..rt.buf.w).map(|x| rt.buf.get(x, y).ch).collect();
                        s += row.trim_end();
                        s.push('\n');
                    }
                    s
                };
                write!(out, "{s}---\n").map_err(|e| e.to_string())?;
            }
            Cmd::Quit => break,
        }
        out.flush().map_err(|e| e.to_string())?;
        // Render after input so hit areas match what the next click sees.
        rt.render();
        if rt.quit {
            break;
        }
    }
    Ok(())
}

fn truecolor() -> bool {
    std::env::var("COLORTERM").is_ok_and(|v| v.contains("truecolor") || v.contains("24bit"))
}

fn diff(s: &mut String, prev: Option<&Buffer>, b: &Buffer, tc: bool) {
    if prev.is_none() {
        s.push_str("\x1b[0m\x1b[2J");
    }
    let mut cur: Option<(Color, Color, u8)> = None;
    let mut pos: Option<(u16, u16)> = None;
    for y in 0..b.h {
        for x in 0..b.w {
            let c = b.get(x, y);
            if prev.is_some_and(|p| p.get(x, y) == c) {
                continue;
            }
            if pos != Some((x, y)) {
                let _ = write!(s, "\x1b[{};{}H", y + 1, x + 1);
            }
            if cur != Some((c.fg, c.bg, c.attrs)) {
                sgr(s, c, tc);
                cur = Some((c.fg, c.bg, c.attrs));
            }
            s.push(c.ch);
            pos = Some((x + 1, y));
        }
    }
    s.push_str("\x1b[0m");
}

fn sgr(s: &mut String, c: Cell, tc: bool) {
    s.push_str("\x1b[0");
    for (bit, code) in [(attr::BOLD, ";1"), (attr::DIM, ";2"), (attr::ITALIC, ";3"), (attr::UNDERLINE, ";4")] {
        if c.attrs & bit != 0 {
            s.push_str(code);
        }
    }
    for (col, base) in [(c.fg, 38), (c.bg, 48)] {
        if let Color::Rgb(r, g, b) = col {
            let _ = if tc {
                write!(s, ";{base};2;{r};{g};{b}")
            } else {
                write!(s, ";{base};5;{}", ansi256(r, g, b))
            };
        }
    }
    s.push('m');
}

/// Nearest xterm-256 index: 6x6x6 cube or the 24-step grey ramp.
fn ansi256(r: u8, g: u8, b: u8) -> u8 {
    const LV: [i32; 6] = [0, 95, 135, 175, 215, 255];
    let q = |v: u8| if v < 48 { 0 } else if v < 115 { 1 } else { (v as usize - 35) / 40 };
    let (qr, qg, qb) = (q(r), q(g), q(b));
    let d = |a: (i32, i32, i32)| {
        let (dr, dg, db) = (a.0 - r as i32, a.1 - g as i32, a.2 - b as i32);
        dr * dr + dg * dg + db * db
    };
    let avg = (r as i32 + g as i32 + b as i32) / 3;
    let gi = ((avg - 8).max(0) / 10).min(23);
    let gv = 8 + gi * 10;
    if d((gv, gv, gv)) < d((LV[qr], LV[qg], LV[qb])) {
        232 + gi as u8
    } else {
        16 + (36 * qr + 6 * qg + qb) as u8
    }
}

/// Renders a buffer as plain ANSI lines (no cursor movement), for printing images inline.
pub fn to_ansi(b: &Buffer, tc: bool) -> String {
    let mut s = String::new();
    for y in 0..b.h {
        let mut cur = None;
        for x in 0..b.w {
            let c = b.get(x, y);
            if cur != Some((c.fg, c.bg, c.attrs)) {
                sgr(&mut s, c, tc);
                cur = Some((c.fg, c.bg, c.attrs));
            }
            s.push(c.ch);
        }
        s.push_str("\x1b[0m\n");
    }
    s
}

/// Decodes a chunk of terminal input into events.
// shortcut: an escape sequence split across two reads is dropped; buffer the tail if that shows up in practice.
pub fn parse(b: &[u8]) -> Vec<Event> {
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == 0x1b && i + 1 < b.len() {
            match b[i + 1] {
                b'[' => {
                    i = csi(b, i + 2, &mut out);
                    continue;
                }
                b'O' if i + 2 < b.len() => {
                    if let Some(k) = final_key(b[i + 2]) {
                        out.push(Event::Key(Key::plain(k)));
                    }
                    i += 3;
                    continue;
                }
                _ => {
                    let (code, n) = single(&b[i + 1..]);
                    if let Some(mut k) = code {
                        k.alt = true;
                        out.push(Event::Key(k));
                    }
                    i += 1 + n;
                    continue;
                }
            }
        }
        let (k, n) = single(&b[i..]);
        out.extend(k.map(Event::Key));
        i += n;
    }
    out
}

fn single(b: &[u8]) -> (Option<Key>, usize) {
    let plain = |c| (Some(Key::plain(c)), 1);
    match b[0] {
        b'\r' | b'\n' => plain(KeyCode::Enter),
        b'\t' => plain(KeyCode::Tab),
        0x7f | 0x08 => plain(KeyCode::Backspace),
        0x1b => plain(KeyCode::Esc),
        c @ 0x01..=0x1a => (Some(Key { code: KeyCode::Char((b'a' + c - 1) as char), ctrl: true, alt: false }), 1),
        0x00 => (Some(Key { code: KeyCode::Char(' '), ctrl: true, alt: false }), 1),
        c @ 0x1c..=0x1f => (Some(Key { code: KeyCode::Char(b"\\]^_"[c as usize - 0x1c] as char), ctrl: true, alt: false }), 1),
        c => {
            let n = match c {
                0xc0..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf7 => 4,
                _ => 1,
            }
            .min(b.len());
            let ch = std::str::from_utf8(&b[..n]).ok().and_then(|s| s.chars().next());
            (ch.map(|c| Key::plain(KeyCode::Char(c))), n)
        }
    }
}

fn final_key(f: u8) -> Option<KeyCode> {
    Some(match f {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        b'Z' => KeyCode::BackTab,
        _ => return None,
    })
}

fn nums(p: &[u8]) -> Vec<u16> {
    std::str::from_utf8(p).unwrap_or("").split(';').map(|n| n.parse().unwrap_or(0)).collect()
}

/// Parses one CSI sequence whose parameters start at `start`; returns the index after it.
fn csi(b: &[u8], start: usize, out: &mut Vec<Event>) -> usize {
    let Some(end) = b[start..].iter().position(|c| (0x40..=0x7e).contains(c)).map(|p| start + p) else {
        return b.len();
    };
    let (params, fin) = (&b[start..end], b[end]);

    if params.first() == Some(&b'<') && (fin == b'M' || fin == b'm') {
        if let [btn, x, y] = nums(&params[1..])[..] {
            let (x, y) = (x.saturating_sub(1), y.saturating_sub(1));
            let btn = btn & !0b1_1100; // strip shift/meta/ctrl
            let kind = match btn {
                64 => Some(MouseKind::ScrollUp),
                65 => Some(MouseKind::ScrollDown),
                _ if btn & 32 != 0 => Some(if btn & 3 == 3 { MouseKind::Move } else { MouseKind::Drag }),
                0 => Some(if fin == b'M' { MouseKind::Down } else { MouseKind::Up }),
                _ => None, // middle/right buttons are not part of the API yet
            };
            out.extend(kind.map(|kind| Event::Mouse { kind, x, y }));
        }
        return end + 1;
    }

    let n = nums(params);
    let code = if fin == b'~' {
        match n.first() {
            Some(1 | 7) => Some(KeyCode::Home),
            Some(4 | 8) => Some(KeyCode::End),
            Some(3) => Some(KeyCode::Delete),
            Some(5) => Some(KeyCode::PageUp),
            Some(6) => Some(KeyCode::PageDown),
            _ => None,
        }
    } else {
        final_key(fin)
    };
    if let Some(code) = code {
        let m = n.get(1).copied().unwrap_or(1).saturating_sub(1);
        out.push(Event::Key(Key { code, ctrl: m & 4 != 0, alt: m & 2 != 0 }));
    }
    end + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kitty_sends_once_and_deletes_gone_images() {
        assert_eq!((base64(b"Man"), base64(b"Ma"), base64(b"M")), ("TWFu".into(), "TWE=".into(), "TQ==".into()));
        let mut b = Buffer::new(10, 4);
        let img = std::sync::Arc::new(crate::image::Image { w: 2, h: 2, px: vec![[255, 0, 0]; 4] });
        b.images.push((Rect::new(1, 1, 2, 1), img));
        let (mut s, mut shown) = (String::new(), vec![]);
        kitty(&mut s, &b, &mut shown);
        assert!(s.contains("a=T,f=24,s=2,v=2,i=1,p=1,c=2,r=1"));
        s.clear();
        kitty(&mut s, &b, &mut shown);
        assert!(s.is_empty(), "unchanged picture is not resent");
        b.images.clear();
        kitty(&mut s, &b, &mut shown);
        assert!(s.contains("a=d,d=I,i=1"));
    }

    fn k(code: KeyCode) -> Event {
        Event::Key(Key::plain(code))
    }

    #[test]
    fn keys_and_utf8() {
        let ev = parse(b"a\x1b[A\x1bOB\x1b[5~\r\x7f\x03\xc3\xa9");
        assert_eq!(
            ev,
            vec![
                k(KeyCode::Char('a')),
                k(KeyCode::Up),
                k(KeyCode::Down),
                k(KeyCode::PageUp),
                k(KeyCode::Enter),
                k(KeyCode::Backspace),
                Event::Key(Key { code: KeyCode::Char('c'), ctrl: true, alt: false }),
                k(KeyCode::Char('é')),
            ]
        );
    }

    #[test]
    fn esc_alt_and_modified_arrow() {
        assert_eq!(parse(b"\x1b"), vec![k(KeyCode::Esc)]);
        assert_eq!(parse(b"\x1bx"), vec![Event::Key(Key { code: KeyCode::Char('x'), ctrl: false, alt: true })]);
        assert_eq!(parse(b"\x1b[1;5C"), vec![Event::Key(Key { code: KeyCode::Right, ctrl: true, alt: false })]);
    }

    #[test]
    fn agent_commands() {
        assert_eq!(command("key ctrl+q"), Some(Cmd::Send(vec![Event::Key(Key { code: KeyCode::Char('q'), ctrl: true, alt: false })])));
        assert_eq!(command("key pgdn"), Some(Cmd::Send(vec![k(KeyCode::PageDown)])));
        assert_eq!(command("drag 3 4"), Some(Cmd::Send(vec![Event::Mouse { kind: MouseKind::Drag, x: 3, y: 4 }])));
        assert_eq!(command("tick 0.5"), Some(Cmd::Tick(0.5)));
        assert_eq!(command("resize 80 24"), Some(Cmd::Resize(80, 24)));
        assert_eq!(command("click 1"), None);
        assert_eq!(command("bogus"), None);
    }

    #[test]
    fn sgr_mouse() {
        let ev = parse(b"\x1b[<0;10;5M\x1b[<32;11;5M\x1b[<35;12;6M\x1b[<0;12;6m\x1b[<65;1;1M");
        let m = |kind, x, y| Event::Mouse { kind, x, y };
        assert_eq!(
            ev,
            vec![
                m(MouseKind::Down, 9, 4),
                m(MouseKind::Drag, 10, 4),
                m(MouseKind::Move, 11, 5),
                m(MouseKind::Up, 11, 5),
                m(MouseKind::ScrollDown, 0, 0),
            ]
        );
    }

    #[test]
    fn diff_skips_unchanged_cells() {
        let mut a = Buffer::new(4, 1);
        a.print(0, 0, "abcd", Default::default(), 4);
        let mut b = a.clone();
        b.set(2, 0, 'X', Default::default());
        let mut s = String::new();
        diff(&mut s, Some(&a), &b, true);
        assert_eq!(s, "\x1b[1;3H\x1b[0mX\x1b[0m");
    }

    #[test]
    fn ansi256_extremes() {
        assert_eq!(ansi256(0, 0, 0), 16);
        assert_eq!(ansi256(255, 255, 255), 231);
        assert_eq!(ansi256(255, 0, 0), 196);
        assert_eq!(ansi256(128, 128, 128), 244);
    }
}
