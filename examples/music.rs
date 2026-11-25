//! A music player in the Spotify mould: rolling title, cover art, transport, seek rail, live spectrum, karaoke lyrics.
//! `cargo run --release --example music -- [file or url]...` plays the queue in order (default: three demo tracks in ~/.cache/halftone/media).
//! Resize to a single row and it becomes a now-playing bar.
//! space pause · ←/→ 10 s · +/- volume · n/p track · l lyrics · t theme · q quit. Needs ffmpeg (yt-dlp for web pages, whisper-cli for lyrics).

use halftone::media::{self, Playback, Transcriber, Video};
use halftone::deck;
use halftone::prelude::*;
use halftone::theme::{Theme, PRESETS};
use std::sync::mpsc::{self, Receiver};
use std::{thread, time::Duration};

const SEEK: u32 = 1;
const VOL: u32 = 2;
const T: Transport = Transport { prev: 3, play: 4, next: 5 };

struct Music {
    queue: Vec<String>,
    /// (title, artist) per queue entry, for the up-next list.
    names: Vec<(String, String)>,
    cur: usize,
    loading: Option<Receiver<Result<Playback, String>>>,
    cover_rx: Option<Receiver<Option<Image>>>,
    pb: Option<Playback>,
    cover: Option<Image>,
    err: Option<String>,
    spec: Spectrum,
    /// Band count the last view asked for.
    bands: usize,
    lyrics: bool,
    tr: Option<Transcriber>,
    tr_err: Option<String>,
    scroll: f32,
    dt: f32,
    from: Theme,
    idx: usize,
    blend: f32,
}

impl Music {
    fn new(queue: Vec<String>) -> Music {
        // Local files only: a web page would need a yt-dlp round trip per entry.
        let names = queue.iter().map(|q| match q.contains("://") {
            true => (q.clone(), String::new()),
            false => media::resolve(q).map_or((q.clone(), String::new()), |s| (s.title, s.artist.unwrap_or_default())),
        }).collect();
        let mut m = Music {
            queue, names, cur: 0, loading: None, cover_rx: None, pb: None, cover: None, err: None, spec: Spectrum::default(), bands: 12,
            lyrics: false, tr: None, tr_err: None, scroll: 0.0, dt: 0.0, from: PRESETS[0], idx: 0, blend: 1.0,
        };
        m.load();
        m
    }

    /// Drops the current track and starts opening `queue[cur]`.
    fn load(&mut self) {
        (self.pb, self.cover, self.cover_rx, self.tr, self.tr_err, self.err, self.scroll) = (None, None, None, None, None, None, 0.0);
        let s = self.queue[self.cur].clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || tx.send(Playback::open(&s, None, true)));
        self.loading = Some(rx);
    }

    fn go(&mut self, d: isize) {
        self.cur = (self.cur as isize + d).rem_euclid(self.queue.len() as isize) as usize;
        self.load();
    }

    fn act(&mut self, f: impl FnOnce(&mut Playback) -> Result<(), String>) {
        if let Some(pb) = &mut self.pb {
            self.err = f(pb).err();
        }
    }

    /// Cover art: embedded art, else one frame from ~10% in; None (no picture, or too slow) leaves the plain tile.
    fn fetch_cover(&mut self) {
        let Some(pb) = &self.pb else { return };
        let (src, at) = (pb.src.video.clone(), pb.len.unwrap_or(0.0) * 0.1);
        let (tx, rx) = mpsc::channel();
        // Embedded album art is a one-frame stream that a seek skips, so fall back to the start.
        thread::spawn(move || _ = tx.send(grab(&src, at).or_else(|| grab(&src, 0.0))));
        self.cover_rx = Some(rx);
    }

    fn source_line(&self) -> String {
        if let Some(a) = self.pb.as_ref().and_then(|p| p.src.artist.clone()) {
            return a;
        }
        let s = &self.queue[self.cur];
        s.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or("Local file").into()
    }

    /// Everything under the title: transport + volume, rail with times, in the column at `x`, width `w`..
    fn controls(&mut self, f: &mut Frame, x: u16, y: u16, w: u16, rail_y: u16) {
        let Some(pb) = &self.pb else { return };
        let (playing, volume, pos, len, prog) = (pb.playing, pb.volume, pb.pos, pb.len, pb.progress());
        transport(f, x, y, playing, T);
        if w >= 24 {
            volume_at(f, Rect::new(x + w - 9, y, 9, 1), volume);
        }
        let (now, total) = (media::clock(pos), len.map_or("live".into(), media::clock));
        let (nw, tw) = (now.chars().count() as u16, total.chars().count() as u16);
        f.buf.print(x, rail_y, &now, fg(theme::muted()), nw);
        f.buf.print(x + w.saturating_sub(tw), rail_y, &total, fg(theme::muted()), tw);
        rail(f, Rect::new(x + nw + 2, rail_y, w.saturating_sub(nw + tw + 4), 1), prog, len, SEEK);
    }

    /// Spectrum, or lyrics when toggled, in `r`.
    fn stage(&mut self, f: &mut Frame, r: Rect) {
        if r.h == 0 || r.w < 4 {
            return;
        }
        if !self.lyrics {
            self.bands = (r.w as usize / 3).min(72);
            self.spec.draw(f.buf, r);
            return;
        }
        let lines = self.tr.as_ref().map(|t| t.lines.lock().unwrap().clone()).unwrap_or_default();
        let pos = self.pb.as_ref().map_or(0.0, |p| p.pos);
        if lines.is_empty() {
            let msg = self.tr_err.clone();
            let (text, y) = (msg.as_deref().unwrap_or("listening…"), r.y + r.h / 2);
            match msg {
                Some(_) => _ = f.buf.print(r.x, y, text, fg(theme::muted()), r.w),
                None => shimmer(f.buf, r.x + r.w.saturating_sub(text.chars().count() as u16) / 2, y, text, f.time, r.w),
            }
        } else {
            lyrics(f.buf, r, &lines, pos, &mut self.scroll, self.dt);
        }
    }
}

/// The first frame ffmpeg gives from `at`, waiting up to 4 s.
fn grab(src: &str, at: f64) -> Option<Image> {
    let mut v = Video::open(src, 160, 160, at).ok()?;
    for _ in 0..80 {
        if let Some(i) = v.frame() {
            return Some(i);
        }
        if v.ended() {
            return v.frame();
        }
        thread::sleep(Duration::from_millis(50));
    }
    None
}

fn volume_at(f: &mut Frame, r: Rect, v: u8) {
    volume(f, r, v as f32 / 100.0, VOL);
}

impl App for Music {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match *ev {
            Event::Tick(dt) => {
                let dt = dt as f32;
                self.dt = dt;
                self.blend += dt / 0.6;
                let t = self.blend.clamp(0.0, 1.0);
                theme::set(self.from.mix(&PRESETS[self.idx], t * t * (3.0 - 2.0 * t)));
                if let Some(Ok(r)) = self.loading.as_ref().map(|rx| rx.try_recv()) {
                    self.loading = None;
                    match r {
                        Ok(pb) => (self.pb, self.err) = (Some(pb), None),
                        Err(e) => self.err = Some(e),
                    }
                    self.fetch_cover();
                }
                if let Some(Ok(c)) = self.cover_rx.as_ref().map(|rx| rx.try_recv()) {
                    (self.cover, self.cover_rx) = (c, None);
                }
                let Some(pb) = &mut self.pb else { return };
                pb.tick(dt as f64);
                let fresh = media::spectrum(&pb.samples(2048), self.bands);
                let (done, pos, src) = (pb.len.is_some_and(|l| pb.pos >= l), pb.pos, pb.src.audio.clone());
                self.spec.update(&fresh, dt);
                if done && self.queue.len() > 1 {
                    return self.go(1);
                }
                if self.lyrics && self.tr.is_none() && self.tr_err.is_none() {
                    match Transcriber::start(&src, pos) {
                        Ok(t) => self.tr = Some(t),
                        Err(e) => self.tr_err = Some(e),
                    }
                }
            }
            Event::Click { id: SEEK, x, rect, .. } | Event::Drag { id: SEEK, x, rect, .. } => {
                self.act(|p| p.seek_to(rail_time(x, rect, p.len.unwrap_or(0.0))));
                self.tr = None; // whisper restarts lazily at the new playhead
            }
            Event::Click { id: VOL, x, rect, .. } | Event::Drag { id: VOL, x, rect, .. } => {
                let v = (deck::volume_value(x, rect) * 100.0).round() as i32;
                if self.pb.as_ref().is_some_and(|p| p.volume as i32 != v) {
                    self.act(|p| p.set_volume(v));
                }
            }
            Event::Click { id, .. } if id == T.play => self.act(Playback::toggle),
            Event::Click { id, .. } if id == T.prev => self.go(-1),
            Event::Click { id, .. } if id == T.next => self.go(1),
            _ => match ev.key() {
                Some(KeyCode::Left) => {
                    self.act(|p| p.seek(-10.0));
                    self.tr = None;
                }
                Some(KeyCode::Right) => {
                    self.act(|p| p.seek(10.0));
                    self.tr = None;
                }
                _ => match ev.ch() {
                    Some(' ') => self.act(Playback::toggle),
                    Some('+' | '=') => self.act(|p| p.set_volume(p.volume as i32 + 10)),
                    Some('-') => self.act(|p| p.set_volume(p.volume as i32 - 10)),
                    Some('n') => self.go(1),
                    Some('p') => self.go(-1),
                    Some('l') => self.lyrics = !self.lyrics,
                    Some('t') => {
                        let t = self.blend.clamp(0.0, 1.0);
                        self.from = self.from.mix(&PRESETS[self.idx], t * t * (3.0 - 2.0 * t));
                        (self.idx, self.blend) = ((self.idx + 1) % PRESETS.len(), 0.0);
                    }
                    Some('q') => ctx.quit(),
                    _ => {}
                },
            },
        }
    }

    fn view(&mut self, f: &mut Frame) {
        let a = f.area;
        if a.w < 24 || a.h == 0 {
            return;
        }
        let Some(pb) = &self.pb else {
            let (msg, y) = (format!("opening {}", self.queue[self.cur]), a.y + a.h / 2);
            let w = (msg.chars().count() as u16).min(a.w - 4);
            match &self.err {
                Some(e) => _ = f.buf.print(a.x + 2, y, e, fg(theme::bad()), a.w - 4),
                None => shimmer(f.buf, a.x + (a.w - w) / 2, y, &msg, f.time, w),
            }
            return;
        };
        let title = pb.src.title.clone();
        let (pos, len) = (pb.pos, pb.len);
        let bold = fg(theme::text()).bold();
        let source = self.source_line();

        if a.h < 4 {
            // BAR: ♪ title · source   ⏮ ⏸ ⏭   spectrum   a / b
            let (y, x0, end) = (a.y + a.h / 2, a.x + 2, a.right() - 2);
            let clock = format!("{} / {}", media::clock(pos), len.map_or("live".into(), media::clock));
            let mut right = end.saturating_sub(clock.chars().count() as u16);
            f.buf.print(right, y, &clock, fg(theme::muted()), end - right);
            if right >= x0 + 40 {
                right -= 14;
                self.bands = 12;
                self.spec.draw(f.buf, Rect::new(right, y, 12, 1));
            }
            if right >= x0 + 30 {
                right -= 13;
                let playing = self.pb.as_ref().is_some_and(|p| p.playing);
                transport(f, right, y, playing, T);
            }
            f.buf.set(x0, y, '♪', fg(theme::accent()).bold());
            let text = self.err.clone().unwrap_or(format!("{title} · {source}"));
            marquee(f.buf, x0 + 2, y, right.saturating_sub(x0 + 4), &text, f.time, bold);
            return;
        }

        let full = a.h >= 16 && a.w >= 70;
        let sh = if full { (a.h / 4).clamp(4, 8) } else { 4 };
        let foot = if full { 2 } else { 1 }; // keys row in FULL
        let top = a.y + 1;
        let body_h = if full { a.h - 2 - sh - foot } else { a.h - 2 };
        let ch = body_h.max(2);
        let cw = (ch * 2).min(a.w * 2 / 5);
        let cov = cover(f.buf, Rect::new(a.x + 3, top, cw, ch), self.cover.as_ref(), f.time);
        let x = a.x + 3 + cw + 3;
        let w = a.right().saturating_sub(x + 3);
        if w < 12 {
            return;
        }
        // Info column, vertically centred on the cover. Spacious: title, source, gap, transport, tip row, rail.
        let tall = ch >= 7;
        let next = if full && ch >= 14 { (self.queue.len() - 1).min(3) as u16 } else { 0 };
        let used = if tall { 6 } else { 4 } + if next > 0 { 3 + next } else { 0 };
        let y0 = cov.y.max(top) + (ch.saturating_sub(used)) / 2;
        let err = self.err.clone();
        marquee(f.buf, x, y0, w, &title, f.time, bold);
        match err {
            Some(e) => _ = f.buf.print(x, y0 + 1, &e, fg(theme::bad()), w),
            None => _ = f.buf.print(x, y0 + 1, &source, fg(theme::muted()), w),
        }
        let (ty, ry) = if tall { (y0 + 3, y0 + 5) } else { (y0 + 2, y0 + 3) };
        self.controls(f, x, ty, w, ry);
        if next > 0 {
            f.buf.print(x, ry + 3, "Up next", fg(theme::muted()), w);
            for i in 0..next {
                let (t, a) = &self.names[(self.cur + 1 + i as usize) % self.queue.len()];
                let e = f.buf.print(x, ry + 4 + i, t, fg(theme::text()), w);
                if !a.is_empty() && e + 3 < x + w {
                    f.buf.print(e + 2, ry + 4 + i, a, fg(theme::faint()), x + w - e - 2);
                }
            }
        }
        if full {
            let stage = Rect::new(a.x + 3, a.bottom() - foot - sh - 1, a.w - 6, sh);
            self.stage(f, stage);
            keys(f.buf, a.x + 3, a.bottom() - 1, &[("space", "pause"), ("n/p", "track"), ("l", "lyrics"), ("t", "theme"), ("q", "quit")]);
        } else if tall {
            let sy = ry + 2;
            let stage = Rect::new(x, sy, w, a.bottom().saturating_sub(sy + 1).min(sh));
            self.stage(f, stage);
        }
    }
}

fn queue() -> Vec<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        return args;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    ["sintel.m4a", "artemis.mp4", "moon.m4a"].iter().map(|n| format!("{home}/.cache/halftone/media/{n}")).collect()
}

halftone::main!(Music::new(queue()));
