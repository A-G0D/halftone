//! One playback, three layouts at once: a status-bar strip, a card, and a panel, all built from
//! the same `deck` parts and sharing one spectrum and one set of hit ids, so play in any toggles all.
//! `cargo run --release --example compose [-- <file or url>]`
//! space pause · ←/→ 10 s · +/- volume · t theme · q quit. Needs ffmpeg. HALFTONE_MUTE=1 for silence.

use halftone::media::{self, Playback};
use halftone::deck;
use halftone::prelude::*;
use halftone::theme::PRESETS;

const BANDS: usize = 24;
const IDS: Transport = Transport { prev: 1, play: 2, next: 3 };
const SEEK: u32 = 4;
const VOL: u32 = 5;

struct Compose {
    pb: Playback,
    spec: Spectrum,
    idx: usize,
    err: Option<String>,
}

impl Compose {
    fn new(src: String) -> Compose {
        let pb = Playback::open(&src, Some((480, 270)), true).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        });
        Compose { pb, spec: Spectrum::default(), idx: 0, err: None }
    }

    fn act(&mut self, f: impl FnOnce(&mut Playback) -> Result<(), String>) {
        self.err = f(&mut self.pb).err();
    }
}

fn label(f: &mut Frame, x: u16, y: u16, w: u16, s: &str) {
    f.buf.print(x, y, s, fg(theme::muted()), w);
}

impl App for Compose {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match *ev {
            Event::Tick(dt) => {
                self.pb.tick(dt);
                self.spec.update(&media::spectrum(&self.pb.samples(2048), BANDS), dt as f32);
            }
            Event::Click { id, x, rect, .. } | Event::Drag { id, x, rect, .. } => match id {
                1 => self.act(|p| p.seek(-10.0)),
                2 => self.act(Playback::toggle),
                3 => self.act(|p| p.seek(10.0)),
                SEEK => self.act(|p| p.seek_to(x as f64 / rect.w.max(1) as f64 * p.len.unwrap_or(0.0))),
                // Drag restarts audio each move; only commit volume on click.
                VOL if matches!(ev, Event::Click { .. }) => {
                    let v = (deck::volume_value(x, rect) * 100.0).round() as i32;
                    self.act(|p| p.set_volume(v));
                }
                _ => {}
            },
            _ => match ev.key() {
                Some(KeyCode::Left) => self.act(|p| p.seek(-10.0)),
                Some(KeyCode::Right) => self.act(|p| p.seek(10.0)),
                _ => match ev.ch() {
                    Some(' ') => self.act(Playback::toggle),
                    Some('+' | '=') => self.act(|p| p.set_volume(p.volume as i32 + 10)),
                    Some('-') => self.act(|p| p.set_volume(p.volume as i32 - 10)),
                    Some('t') => {
                        self.idx = (self.idx + 1) % PRESETS.len();
                        theme::set(PRESETS[self.idx]);
                    }
                    Some('q') => ctx.quit(),
                    _ => {}
                },
            },
        }
    }

    fn view(&mut self, f: &mut Frame) {
        let a = f.area;
        if a.w < 50 || a.h < 22 {
            f.buf.print(a.x, a.y, "resize: need 50x22", fg(theme::muted()), a.w);
            return;
        }
        let (x0, w) = (a.x + 2, a.w - 4);
        let pb = &self.pb;
        let (title, text, accent) = (&pb.src.title, fg(theme::text()).bold(), theme::accent());
        let clock = format!("{} / {}", media::clock(pb.pos), pb.len.map_or("live".into(), media::clock));
        let y = a.y + 1;

        // 1. Bar: ♪ title · transport · spectrum · clock on one row.
        label(f, x0, y, w, "bar · marquee + transport + spectrum + clock");
        let y = y + 1;
        f.buf.set(x0, y, '♪', fg(if pb.playing { accent } else { theme::muted() }));
        let tail = 11 + 2 + 12 + 2 + clock.len() as u16;
        let tw = w - 2 - tail - 1;
        marquee(f.buf, x0 + 2, y, tw, title, f.time, text);
        let tx = x0 + 2 + tw + 1;
        transport(f, tx, y, pb.playing, IDS);
        self.spec.draw(f.buf, Rect::new(tx + 13, y, 12, 1));
        f.buf.print(tx + 27, y, &clock, fg(theme::muted()), clock.len() as u16);

        // 2. Card: cover left; title, transport + volume, seek rail right.
        let y = y + 2;
        label(f, x0, y, w, "card · cover + marquee + transport + rail + volume");
        let y = y + 1;
        cover(f.buf, Rect::new(x0, y, 14, 7), pb.frame.as_ref(), f.time);
        let cx = x0 + 16;
        marquee(f.buf, cx, y + 1, w - 16, title, f.time, text);
        transport(f, cx, y + 3, pb.playing, IDS);
        volume(f, Rect::new(cx + 13, y + 3, 5.min(w - 29), 1), pb.volume as f32 / 100.0, VOL);
        rail(f, Rect::new(cx, y + 5, w - 16, 1), pb.progress(), pb.len, SEEK);

        // 3. Panel: video frame beside a tall spectrum with peaks.
        let y = y + 8;
        label(f, x0, y, w, "panel · image::draw + spectrum (peaks)");
        let h = a.bottom() - 2 - (y + 1);
        let half = w / 2;
        match &pb.frame {
            Some(img) => _ = halftone::image::draw(f.buf, Rect::new(x0, y + 1, half, h), img, Opts::default()),
            None => _ = cover(f.buf, Rect::new(x0, y + 1, half, h), None, f.time),
        }
        self.spec.draw(f.buf, Rect::new(x0 + half + 1, y + 1, w - half - 1, h));

        match &self.err {
            Some(e) => _ = f.buf.print(x0, a.bottom() - 1, e, fg(theme::bad()), w),
            None => _ = keys(f.buf, x0, a.bottom() - 1, &[("space", "pause"), ("←→", "seek"), ("+-", "volume"), ("t", "theme"), ("q", "quit")]),
        }
    }
}

halftone::main!(Compose::new(std::env::args().nth(1).unwrap_or_else(|| {
    format!("{}/.cache/halftone/media/artemis.mp4", std::env::var("HOME").unwrap_or_default())
})));
