//! Dynamic-Island pill with four Live Activities: music, timer, build, agent.
//! `cargo run --release --example island [-- <media file>]` (music is synthetic without one; real needs ffmpeg).
//! space expand · tab next · 1-4 activities · q quit. HALFTONE_SCRIPT=1 plays a ~17 s looping timeline for recording.

use halftone::media::{self, Playback};
use halftone::prelude::*;

const ID: u32 = 1;
const BANDS: usize = 24;
const LOOP: f64 = 17.0;

#[derive(Clone, Copy)]
enum Do {
    Toggle,
    Next,
    Count(usize),
}

// Compact solo, expand, collapse, grow to the minimal pill, rotate, expand the agent.
const SCRIPT: [(f64, Do); 9] = [
    (2.0, Do::Toggle),
    (5.0, Do::Toggle),
    (6.5, Do::Count(2)),
    (8.0, Do::Count(3)),
    (9.5, Do::Next),
    (11.0, Do::Next),
    (12.0, Do::Count(4)),
    (12.4, Do::Next),
    (13.5, Do::Toggle),
];

struct Demo {
    isl: Island,
    count: usize,
    pb: Option<Playback>,
    spec: Spectrum,
    t: f64,
    script: bool,
    step: usize,
    dt: f32,
}

impl Demo {
    fn new(src: Option<String>) -> Demo {
        let pb = src.map(|s| {
            Playback::open(&s, None, true).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1)
            })
        });
        let script = std::env::var_os("HALFTONE_SCRIPT").is_some();
        Demo { isl: Island::default(), count: if script { 1 } else { 4 }, pb, spec: Spectrum::default(), t: 0.0, script, step: 0, dt: 0.0 }
    }

    fn apply(&mut self, d: Do) {
        match d {
            Do::Toggle => self.isl.toggle(),
            Do::Next => self.isl.next(),
            Do::Count(n) => self.count = n,
        }
    }

    fn acts(&self) -> Vec<Activity> {
        let t = self.t;
        let (pos, title) = match &self.pb {
            Some(p) => (p.pos, p.src.title.clone()),
            None => (t, "Halftone Nights".into()),
        };
        let left = (300.0 - t).max(0.0);
        let build = (t / 12.0).fract() as f32;
        let a = |icon, tint: u32, lead: &str, trail: String, title: &str, detail: &str, progress, live| Activity {
            icon, tint: Color::rgb(tint), lead: lead.into(), trail, title: title.into(), detail: detail.into(), progress, live,
        };
        vec![
            a('♪', 0xff375f, "", media::clock(pos), &title, "now playing", None, true),
            a('◷', 0xff9f0a, "Timer", media::clock(left), "Timer", "tea is ready in", Some((left / 300.0) as f32), false),
            a('⚙', 0x30d158, "cargo", format!("{:.0}%", build * 100.0), "cargo test", "running 142 tests", Some(build), true),
            a('✻', 0xd97757, "claude", media::clock(t), "Claude Code", "editing src/island.rs", Some((t / 20.0).fract() as f32), true),
        ]
    }
}

impl App for Demo {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match *ev {
            Event::Tick(dt) => {
                (self.t, self.dt) = (self.t + dt, self.dt + dt as f32);
                if self.script {
                    if self.t >= LOOP {
                        (self.t, self.step, self.count) = (0.0, 0, 1);
                        self.isl.expanded = false;
                        self.isl.active = 0;
                    }
                    while let Some(&(_, d)) = SCRIPT.get(self.step).filter(|(at, _)| *at <= self.t) {
                        self.step += 1;
                        self.apply(d);
                    }
                }
                let fresh = match &mut self.pb {
                    Some(p) => {
                        p.tick(dt);
                        media::spectrum(&p.samples(2048), BANDS)
                    }
                    None => (0..BANDS)
                        .map(|i| {
                            let (x, t) = (i as f64, self.t);
                            (0.5 + 0.35 * (t * 3.1 + x * 0.7).sin() + 0.15 * (t * 7.3 + x * 1.9).sin()).max(0.05) as f32 * (1.0 - x as f32 / 40.0)
                        })
                        .collect(),
                };
                self.spec.update(&fresh, dt as f32);
            }
            Event::Click { id: ID, .. } => self.isl.toggle(),
            Event::Click { id, .. } if id == ID + 1 => self.isl.next(),
            _ => match (ev.key(), ev.ch()) {
                (Some(KeyCode::Tab), _) => self.isl.next(),
                (_, Some(' ')) => self.isl.toggle(),
                (_, Some(c @ '1'..='4')) => self.count = c as usize - '0' as usize,
                (_, Some('q')) => ctx.quit(),
                _ => {}
            },
        }
    }

    fn view(&mut self, f: &mut Frame) {
        let a = f.area;
        let all = self.acts();
        let acts = &all[..self.count.min(all.len())];
        let dt = std::mem::take(&mut self.dt); // last tick, already HALFTONE_SPEED-scaled
        self.isl.draw(f, Rect::new(a.x, a.y + 1, a.w, a.h.saturating_sub(1)), acts, dt, ID);
        // The music activity leaves a free row in its expanded pill for the spectrum.
        if self.isl.active == 0 && self.isl.slot.w > 0 {
            self.spec.draw(f.buf, self.isl.slot);
        }
        if a.h > 2 && !self.script {
            keys(f.buf, a.x + 2, a.bottom() - 1, &[("space", "expand"), ("tab", "next"), ("1-4", "activities"), ("q", "quit")]);
        }
    }
}

halftone::main!(Demo::new(std::env::args().nth(1)));
