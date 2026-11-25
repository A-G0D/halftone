//! Live transcription: plays a file or web page and writes what it hears as it goes.
//! `cargo run --release --example transcribe -- <file or url>`
//! space pause · ←/→ 10 s · s save .srt · t theme · q quit.
//! Needs ffmpeg, whisper-cli (whisper.cpp) and a model (see `media::whisper_model`).

use halftone::deck::{self, Line, Transport};
use halftone::media::{self, Playback, Transcriber};
use halftone::prelude::*;
use halftone::theme::{Theme, PRESETS};
use std::sync::mpsc::{self, Receiver};
use std::thread;

const SEEK: u32 = 1;
const BTN: Transport = Transport { prev: 2, play: 3, next: 4 };
// "Steamboat Willie" (1928), public domain in the US since 2024.
const DEMO: &str = "https://www.youtube.com/watch?v=BBgghnQF6E4";

struct Scribe {
    src: String,
    loading: Option<Receiver<Result<Playback, String>>>,
    pb: Option<Playback>,
    caps: Option<Transcriber>,
    err: Option<String>,
    note: Option<(String, f32)>,
    /// Every line heard so far, kept across seeks; sorted by start time.
    lines: Vec<Line>,
    seen: usize,
    /// Eased lyric position, and the last tick's dt for it.
    scroll: f32,
    dt: f32,
    from: Theme,
    idx: usize,
    blend: f32,
}

impl Scribe {
    fn new(src: String) -> Scribe {
        let s = src.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || tx.send(Playback::open(&s, None, false)));
        Scribe { src, loading: Some(rx), pb: None, caps: None, err: None, note: None, lines: vec![], seen: 0, scroll: 0.0, dt: 0.016, from: PRESETS[0], idx: 0, blend: 1.0 }
    }

    /// Restarts whisper at the playhead; anything it already heard stays in `lines`.
    fn listen(&mut self) {
        let Some(pb) = &self.pb else { return };
        (self.caps, self.seen) = (None, 0);
        if pb.playing {
            match Transcriber::start(&pb.src.audio, pb.pos) {
                Ok(t) => self.caps = Some(t),
                Err(e) => self.err = Some(e),
            }
        }
    }

    fn act(&mut self, f: impl FnOnce(&mut Playback) -> Result<(), String>) {
        if let Some(pb) = &mut self.pb {
            self.err = f(pb).err();
        }
        self.listen();
    }

    fn save(&mut self) {
        let path = "transcript.srt";
        let msg = match std::fs::write(path, srt(&self.lines)) {
            Ok(()) => format!("saved {} lines to {path}", self.lines.len()),
            Err(e) => format!("could not save {path}: {e}"),
        };
        self.note = Some((msg, 3.0));
    }
}

/// SubRip: each line runs until the next one starts (or its own end, whichever is sooner).
fn srt(lines: &[Line]) -> String {
    let ts = |t: f64| {
        let ms = (t.max(0.0) * 1000.0) as u64;
        format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
    };
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        let end = lines.get(i + 1).map_or(l.end, |n| n.start.min(l.end));
        out += &format!("{}\n{} --> {}\n{}\n\n", i + 1, ts(l.start), ts(end), l.text());
    }
    out
}

impl App for Scribe {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match *ev {
            Event::Tick(dt) => {
                let dt = dt as f32;
                self.blend += dt / 0.6;
                self.dt = dt;
                let t = self.blend.clamp(0.0, 1.0);
                theme::set(self.from.mix(&PRESETS[self.idx], t * t * (3.0 - 2.0 * t)));
                if let Some((_, left)) = &mut self.note {
                    *left -= dt;
                    if *left <= 0.0 {
                        self.note = None;
                    }
                }
                if let Some(rx) = &self.loading {
                    if let Ok(r) = rx.try_recv() {
                        self.loading = None;
                        match r {
                            Ok(pb) => self.pb = Some(pb),
                            Err(e) => self.err = Some(e),
                        }
                        self.listen();
                    }
                }
                if let Some(pb) = &mut self.pb {
                    pb.tick(dt as f64);
                }
                // Merge new lines in time order, replacing any we heard before at the same spot.
                if let Some(caps) = &self.caps {
                    let got = caps.lines.lock().unwrap();
                    for l in got.iter().skip(self.seen) {
                        self.lines.retain(|u| (u.start - l.start).abs() > 1.0);
                        let at = self.lines.partition_point(|u| u.start < l.start);
                        self.lines.insert(at, l.clone());
                    }
                    self.seen = got.len();
                }
            }
            Event::Click { id: SEEK, x, rect, .. } | Event::Drag { id: SEEK, x, rect, .. } => {
                self.act(|p| p.seek_to(deck::rail_time(x, rect, p.len.unwrap_or(0.0))));
            }
            Event::Click { id, .. } if id == BTN.prev => self.act(|p| p.seek(-10.0)),
            Event::Click { id, .. } if id == BTN.play => self.act(Playback::toggle),
            Event::Click { id, .. } if id == BTN.next => self.act(|p| p.seek(10.0)),
            _ => match ev.key() {
                Some(KeyCode::Left) => self.act(|p| p.seek(-10.0)),
                Some(KeyCode::Right) => self.act(|p| p.seek(10.0)),
                _ => match ev.ch() {
                    Some(' ') => self.act(Playback::toggle),
                    Some('s') => self.save(),
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
        if a.w < 40 || a.h < 14 {
            return;
        }
        let Some(pb) = &self.pb else {
            let msg = format!("opening {}", self.src);
            let w = (msg.chars().count() as u16).min(a.w - 4);
            let (x, y) = (a.x + (a.w - w) / 2, a.y + a.h / 2);
            match &self.err {
                Some(e) => _ = f.buf.print(a.x + 2, y, e, fg(theme::bad()), a.w - 4),
                None => shimmer(f.buf, x, y, &msg, f.time, w),
            }
            return;
        };
        let x0 = a.x + 3;
        deck::marquee(f.buf, x0, a.y + 1, a.w - 6, &pb.src.title, f.time, fg(theme::text()).bold());

        // Karaoke: the current word wipes to accent as it is spoken.
        let ly = Rect::new(x0, a.y + 3, a.w - 6, a.h - 9);
        if self.lines.is_empty() && self.caps.is_some() {
            shimmer(f.buf, ly.x + (ly.w - 10) / 2, ly.y + ly.h * 2 / 5, "listening…", f.time, 10);
        }
        deck::lyrics(f.buf, ly, &self.lines, pb.pos, &mut self.scroll, self.dt);

        let tw = 11;
        deck::transport(f, a.x + (a.w - tw) / 2, a.bottom() - 5, pb.playing, BTN);
        let y = a.bottom() - 3;
        let now = media::clock(pb.pos);
        let total = pb.len.map_or("live".into(), media::clock);
        f.buf.print(x0, y, &now, fg(theme::muted()), 10);
        let rail = Rect::new(x0 + now.len() as u16 + 2, y, a.w.saturating_sub(now.len() as u16 + total.len() as u16 + 10), 1);
        deck::rail(f, rail, pb.progress(), pb.len, SEEK);
        f.buf.print(rail.right() + 2, y, &total, fg(theme::muted()), 10);
        let kx = keys(f.buf, x0, a.bottom() - 1, &[("space", "pause"), ("←→", "seek"), ("s", "save .srt"), ("t", "theme"), ("q", "quit")]);
        if let Some((n, _)) = &self.note {
            f.buf.print(kx + 2, a.bottom() - 1, n, fg(theme::good()), a.right().saturating_sub(kx + 2));
        }
        if let Some(e) = &self.err {
            f.buf.print(x0, a.y, e, fg(theme::bad()), a.w - 6);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_ends_at_next_line() {
        let l = |t: f64, s: &str| Line { start: t, end: t + 5.0, words: vec![deck::Word { start: t, end: t + 1.0, text: s.into() }] };
        let s = srt(&[l(0.0, "a"), l(3.5, "b")]);
        assert!(s.starts_with("1\n00:00:00,000 --> 00:00:03,500\na\n\n2\n00:00:03,500 --> 00:00:08,500\nb"));
    }
}

halftone::main!(Scribe::new(std::env::args().nth(1).unwrap_or_else(|| DEMO.into())));
