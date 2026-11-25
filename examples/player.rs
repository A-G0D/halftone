//! A video player: local files, or any page yt-dlp knows (YouTube included).
//! `cargo run --release --example player -- <file or url>`
//! space pause · ←/→ 10 s · +/- volume · m draw mode · c captions · t theme · q quit.
//! Mouse: click the rail to seek, click/drag volume, click ⏮ ⏭ ▶ CC; the control bar auto-hides after 2.5 s.
//! Needs ffmpeg (and yt-dlp for web pages); captions also need whisper-cli and a model.

use halftone::deck::{self, Spectrum};
use halftone::image;
use halftone::media::{self, Playback, Transcriber};
use halftone::prelude::*;
use halftone::theme::{Theme, PRESETS};
use std::sync::mpsc::{self, Receiver};
use std::thread;

const MODES: [(Mode, &str); 4] = [(Mode::Half, "half"), (Mode::Braille, "braille"), (Mode::Blocks, "blocks"), (Mode::Ascii, "ascii")];
const SEEK: u32 = 1;
const PREV: u32 = 2;
const PLAY: u32 = 3;
const NEXT: u32 = 4;
const VOL: u32 = 5;
const CC: u32 = 6;
// Big Buck Bunny, CC BY 3.0 Blender Foundation.
const DEMO: &str = "https://www.youtube.com/watch?v=aqz-KE-bpKQ";

struct Player {
    src: String,
    loading: Option<Receiver<Result<Playback, String>>>,
    pb: Option<Playback>,
    err: Option<String>,
    mode: usize,
    size: (u16, u16),
    /// Seconds since the last input; the HUD fades out once it passes 2.5.
    idle: f32,
    hud: f32,
    caps: Option<Transcriber>,
    spec: Spectrum,
    from: Theme,
    idx: usize,
    blend: f32,
}

impl Player {
    fn new(src: String) -> Player {
        let s = src.clone();
        let (w, h) = halftone::term::size();
        let (tx, rx) = mpsc::channel();
        // yt-dlp can take a few seconds; resolve off the UI thread.
        thread::spawn(move || tx.send(Playback::open(&s, Some((w as u32, h as u32 * 2)), true)));
        Player { src, loading: Some(rx), pb: None, err: None, mode: 0, size: (0, 0), idle: 0.0, hud: 1.0, caps: None, spec: Spectrum::default(), from: PRESETS[0], idx: 0, blend: 1.0 }
    }

    /// Video pixels for the current mode and stage: two rows per cell for half blocks, 2×4 for braille.
    fn pixels(&self) -> (u32, u32) {
        let (w, h) = (self.size.0 as u32, self.size.1 as u32);
        match MODES[self.mode].0 {
            Mode::Braille => (w * 2, h * 4),
            _ => (w, h * 2),
        }
    }

    fn sync(&mut self) {
        let px = self.pixels();
        if let Some(pb) = &mut self.pb {
            let r = if px.0 == 0 { Ok(()) } else { pb.resize(px.0, px.1) };
            self.err = r.err();
        }
    }

    fn act(&mut self, f: impl FnOnce(&mut Playback) -> Result<(), String>) {
        if let Some(pb) = &mut self.pb {
            if let Err(e) = f(pb) {
                self.err = Some(e);
            }
            // Captions follow the clock, so a seek restarts them from the new spot.
            if self.caps.is_some() {
                self.caps = Transcriber::start(&pb.src.audio, pb.pos).ok();
            }
        }
    }

    fn captions(&mut self) {
        let Some(pb) = &self.pb else { return };
        self.caps = match self.caps.take() {
            Some(_) => None,
            None => match Transcriber::start(&pb.src.audio, pb.pos) {
                Ok(t) => Some(t),
                Err(e) => {
                    self.err = Some(e);
                    None
                }
            },
        };
    }
}

impl App for Player {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match *ev {
            Event::Tick(dt) => {
                let dt = dt as f32;
                self.idle += dt;
                let target = if self.idle < 2.5 || self.pb.as_ref().is_some_and(|p| !p.playing) { 1.0 } else { 0.0 };
                self.hud += (target - self.hud) * (dt * 8.0).min(1.0);
                self.blend += dt / 0.6;
                let t = self.blend.clamp(0.0, 1.0);
                theme::set(self.from.mix(&PRESETS[self.idx], t * t * (3.0 - 2.0 * t)));
                if let Some(rx) = &self.loading {
                    if let Ok(r) = rx.try_recv() {
                        self.loading = None;
                        match r {
                            Ok(pb) => self.pb = Some(pb),
                            Err(e) => self.err = Some(e),
                        }
                        self.sync();
                    }
                }
                if let Some(pb) = &mut self.pb {
                    pb.tick(dt as f64);
                    self.spec.update(&media::spectrum(&pb.samples(2048), 12), dt);
                }
                return;
            }
            Event::Resize(..) => return,
            Event::Mouse { .. } => self.idle = 0.0,
            Event::Click { id: SEEK, x, rect, .. } | Event::Drag { id: SEEK, x, rect, .. } => {
                let r = x as f64 / rect.w.max(1) as f64;
                self.act(|p| p.seek_to(r * p.len.unwrap_or(0.0)));
            }
            Event::Click { id: VOL, x, rect, .. } | Event::Drag { id: VOL, x, rect, .. } => {
                let v = (deck::volume_value(x, rect) * 100.0).round() as i32;
                self.act(|p| p.set_volume(v));
            }
            Event::Click { id: PREV, .. } => self.act(|p| p.seek(-10.0)),
            Event::Click { id: NEXT, .. } => self.act(|p| p.seek(10.0)),
            Event::Click { id: PLAY, .. } => self.act(Playback::toggle),
            Event::Click { id: CC, .. } => self.captions(),
            _ => {}
        }
        if ev.key().is_some() {
            self.idle = 0.0;
        }
        match ev.key() {
            Some(KeyCode::Left) => self.act(|p| p.seek(-10.0)),
            Some(KeyCode::Right) => self.act(|p| p.seek(10.0)),
            _ => match ev.ch() {
                Some(' ') => self.act(Playback::toggle),
                Some('+' | '=') => self.act(|p| p.set_volume(p.volume as i32 + 10)),
                Some('-') => self.act(|p| p.set_volume(p.volume as i32 - 10)),
                Some('m') => {
                    self.mode = (self.mode + 1) % MODES.len();
                    self.sync();
                }
                Some('c') => self.captions(),
                Some('t') => {
                    let t = self.blend.clamp(0.0, 1.0);
                    self.from = self.from.mix(&PRESETS[self.idx], t * t * (3.0 - 2.0 * t));
                    (self.idx, self.blend) = ((self.idx + 1) % PRESETS.len(), 0.0);
                }
                Some('q') => ctx.quit(),
                _ => {}
            },
        }
    }

    fn view(&mut self, f: &mut Frame) {
        let a = f.area;
        if a.w < 24 || a.h < 8 {
            return;
        }
        if (a.w, a.h) != self.size {
            self.size = (a.w, a.h);
            self.sync();
        }

        let Some(pb) = &self.pb else {
            // Loading: a light band sweeps across the source name.
            let msg = self.err.clone().unwrap_or_else(|| format!("opening {}", self.src));
            let w = (msg.chars().count() as u16).min(a.w - 4);
            let (x, y) = (a.x + (a.w - w) / 2, a.y + a.h / 2);
            match &self.err {
                Some(e) => _ = f.buf.print(x, y, e, fg(theme::bad()), w),
                None => shimmer(f.buf, x, y, &msg, f.time, w),
            }
            return;
        };

        if let Some(img) = &pb.frame {
            image::draw(f.buf, a, img, Opts { mode: MODES[self.mode].0, ..Opts::default() });
        }

        // Control bar: bottom two rows over the picture; cells blend from the scrim back to the picture by `hud`.
        let k = self.hud;
        let band = Rect::new(a.x, a.bottom() - 2, a.w, 2);

        // Captions: one karaoke line above the bar, current word in accent.
        if let Some(caps) = &self.caps {
            let lines = caps.lines.lock().unwrap();
            let y = band.y - 2;
            match deck::current(&lines, pb.pos).map(|i| &lines[i]).filter(|l| pb.pos < l.end + 2.0) {
                Some(l) => {
                    let w = (l.text().chars().count() as u16).min(a.w - 6);
                    let mut x = a.x + (a.w - w) / 2;
                    f.buf.fill(Rect::new(x - 1, y, w + 2, 1), theme::bg());
                    for wd in &l.words {
                        let st = if pb.pos >= wd.end { fg(theme::text()) } else if pb.pos >= wd.start { fg(theme::accent()).bold() } else { fg(theme::muted()) };
                        if x >= a.x + (a.w + w) / 2 {
                            break;
                        }
                        x = f.buf.print(x, y, &wd.text, st.bg(theme::bg()), (a.x + (a.w + w) / 2).saturating_sub(x)) + 1;
                    }
                }
                None if lines.is_empty() => {
                    f.buf.print(a.x + 2, y, "listening…", fg(theme::muted()), 20);
                }
                None => {}
            }
        }

        if k >= 0.02 {
            let under: Vec<Cell> = (band.y..band.bottom()).flat_map(|y| (band.x..band.right()).map(move |x| (x, y))).map(|(x, y)| f.buf.get(x, y)).collect();
            let scrim = |f: &mut Frame| {
                for y in band.y..band.bottom() {
                    let depth = k * (0.6 + 0.25 * (y - band.y) as f32);
                    for x in band.x..band.right() {
                        if let Some(c) = f.buf.cell(x, y) {
                            c.bg = c.bg.mix(theme::bg(), depth);
                            c.fg = c.fg.mix(theme::bg(), depth);
                        }
                    }
                }
            };
            scrim(f);
            let scrimmed: Vec<Cell> = (band.y..band.bottom()).flat_map(|y| (band.x..band.right()).map(move |x| (x, y))).map(|(x, y)| f.buf.get(x, y)).collect();
            let (x0, ry) = (a.x + 2, band.y);

            // Row 0: seek rail with hover time tooltip.
            deck::rail(f, Rect::new(x0, ry, a.w - 4, 1), pb.progress(), pb.len, SEEK);

            // Row 1: transport, clock, title, then spectrum / volume / badges right-aligned.
            let y = ry + 1;
            let x = deck::transport(f, x0, y, pb.playing, deck::Transport { prev: PREV, play: PLAY, next: NEXT }) + 2;
            let clock = format!("{} / {}", media::clock(pb.pos), pb.len.map_or("live".into(), media::clock));
            let x = f.buf.print(x, y, &clock, fg(theme::text()), 24) + 2;
            let (pw, ph) = self.pixels();
            let mode = format!("{} {}×{}", MODES[self.mode].1.to_uppercase(), pw, ph);
            let mut rx = a.right().saturating_sub(2 + mode.chars().count() as u16 + 2);
            if rx > x {
                deck::badge(f.buf, rx, y, &mode);
                let cc = rx.saturating_sub(5);
                if cc > x {
                    rx = cc;
                    if self.caps.is_some() {
                        deck::badge(f.buf, rx, y, "CC");
                    } else {
                        f.buf.print(rx + 1, y, "CC", fg(if f.hover(Rect::new(rx, y, 4, 1)) { theme::text() } else { theme::muted() }), 2);
                    }
                    f.hit(Rect::new(rx, y, 4, 1), CC);
                }
                if rx > x + 6 {
                    rx -= 6;
                    deck::volume(f, Rect::new(rx, y, 5, 1), pb.volume as f32 / 100.0, VOL);
                }
                if rx > x + 14 {
                    rx -= 13;
                    self.spec.draw(f.buf, Rect::new(rx, y, 12, 1));
                }
            }

            // Title fills what the right-hand cluster leaves.
            if rx > x + 8 {
                f.buf.print(x + 1, y, &pb.src.title, fg(theme::muted()), rx - x - 4);
            }

            for (i, (u, s)) in under.iter().zip(&scrimmed).enumerate() {
                let (x, y) = (band.x + i as u16 % band.w, band.y + i as u16 / band.w);
                let c = f.buf.get(x, y);
                if c == *s {
                    continue;
                }
                if let Some(d) = f.buf.cell(x, y) {
                    d.fg = u.fg.mix(c.fg, k);
                    d.bg = u.bg.mix(c.bg, k);
                    if k < 0.5 {
                        d.ch = u.ch;
                    }
                }
            }
        }
        if let Some(e) = &self.err {
            f.buf.print(a.x + 2, a.y, e, fg(theme::bad()), a.w - 4);
        }
    }
}

halftone::main!(Player::new(std::env::args().nth(1).unwrap_or_else(|| DEMO.into())));
