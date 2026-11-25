//! One small player, restyled live. `cargo run --example themes`.
//! Click a dot or press `t` to change theme, space plays, `j`/`k` pick a track.

use halftone::prelude::*;
use halftone::theme::{Theme, PRESETS};

const DOT: u32 = 10;
const TRACK: u32 = 20;
const TRACKS: [(&str, &str, f32); 3] = [("Low Tide", "Mira Holm", 228.0), ("Glass Harbour", "Mira Holm", 191.0), ("Undertow", "Mira Holm", 254.0)];

struct Player {
    idx: usize,
    from: Theme,
    blend: f32,
    idle: f32,
    track: usize,
    pos: f32,
    playing: bool,
    level: f32,
}

impl Player {
    fn theme(&mut self, i: usize) {
        self.from = self.current();
        (self.idx, self.blend, self.idle) = (i % PRESETS.len(), 0.0, 0.0);
    }
    fn current(&self) -> Theme {
        let t = self.blend.clamp(0.0, 1.0);
        self.from.mix(&PRESETS[self.idx], t * t * (3.0 - 2.0 * t))
    }
    fn pick(&mut self, i: usize) {
        (self.track, self.pos, self.playing) = (i % TRACKS.len(), 0.0, true);
    }
}

fn clock(s: f32) -> String {
    format!("{}:{:02}", s as u32 / 60, s as u32 % 60)
}

impl App for Player {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match *ev {
            Event::Tick(dt) => {
                let dt = dt as f32;
                self.blend += dt / 0.6;
                self.idle += dt;
                if self.idle > 3.2 {
                    self.theme(self.idx + 1);
                }
                theme::set(self.current());
                let target = if self.playing { 1.0 } else { 0.0 };
                self.level += (target - self.level) * (dt * 6.0).min(1.0);
                if self.playing {
                    self.pos += dt;
                    if self.pos >= TRACKS[self.track].2 {
                        self.pick(self.track + 1);
                    }
                }
            }
            Event::Click { id, .. } if (DOT..DOT + 4).contains(&id) => self.theme((id - DOT) as usize),
            Event::Click { id, .. } if (TRACK..TRACK + 3).contains(&id) => self.pick((id - TRACK) as usize),
            _ => match ev.key() {
                Some(KeyCode::Char(' ')) => self.playing = !self.playing,
                Some(KeyCode::Char('t')) => self.theme(self.idx + 1),
                Some(KeyCode::Down | KeyCode::Char('j')) => self.pick(self.track + 1),
                Some(KeyCode::Up | KeyCode::Char('k')) => self.pick(self.track + TRACKS.len() - 1),
                Some(KeyCode::Char('q')) => ctx.quit(),
                _ => {}
            },
        }
    }

    fn view(&mut self, f: &mut Frame) {
        let a = f.area;
        let (w, h) = (a.w.min(56), a.h.min(18));
        if w < 24 || h < 16 {
            return;
        }
        let card = Rect::new(a.x + (a.w - w) / 2, a.y + (a.h - h) / 2, w, h);
        f.buf.fill(card, theme::panel());
        let r = card.inset(3, 1);
        let (x, y) = (r.x, r.y);

        f.buf.print(x, y, "now playing", fg(theme::faint()), r.w);
        for i in 0..PRESETS.len() {
            let dx = r.right() - (PRESETS.len() - i) as u16 * 2 + 1;
            let on = i == self.idx;
            let c = if on { theme::accent() } else if f.hover(Rect::new(dx, y, 1, 1)) { theme::muted() } else { theme::faint() };
            f.buf.set(dx, y, if on { '●' } else { '○' }, fg(c));
            f.hit(Rect::new(dx, y, 1, 1), DOT + i as u32);
        }

        let (name, artist, len) = TRACKS[self.track];
        f.buf.print(x, y + 2, name, fg(theme::text()).bold(), r.w);
        f.buf.print(x, y + 3, &format!("{artist} — Saltwater"), fg(theme::muted()), r.w);

        // Synthetic spectrum: louder lows, two beating sines per bar, sinks to silence on pause.
        let t = f.time as f32;
        let bars: Vec<f32> = (0..r.w)
            .map(|i| {
                let k = i as f32 / r.w as f32;
                let shape = 0.25 + 0.75 * (1.0 - k).powf(0.7);
                let wob = 0.55 + 0.45 * (t * 5.3 + i as f32 * 0.9).sin() * (t * 2.1 + i as f32 * 0.37).cos();
                (shape * wob * self.level).max(0.03)
            })
            .collect();
        sparkline(f.buf, Rect::new(x, y + 5, r.w, 5), &bars, 1.0, theme::accent());

        let (now, end) = (clock(self.pos), clock(len));
        f.buf.print(x, y + 11, &now, fg(theme::muted()), r.w);
        f.buf.print(r.right() - end.len() as u16, y + 11, &end, fg(theme::muted()), r.w);
        let bar = Rect::new(x + now.len() as u16 + 2, y + 11, r.w - now.len() as u16 - end.len() as u16 - 4, 1);
        gauge(f.buf, bar, self.pos / len, theme::accent(), theme::accent());

        for (i, (title, _, l)) in TRACKS.iter().enumerate() {
            let row = Rect::new(x, y + 13 + i as u16, r.w, 1);
            let on = i == self.track;
            let c = if on { theme::text() } else if f.hover(row) { theme::muted() } else { theme::faint() };
            if on {
                f.buf.set(x, row.y, '▍', fg(theme::accent()));
            }
            f.buf.print(x + 2, row.y, title, fg(c), r.w - 2);
            let d = clock(*l);
            f.buf.print(r.right() - d.len() as u16, row.y, &d, fg(c), r.w);
            f.hit(row, TRACK + i as u32);
        }
    }
}

halftone::main!(Player { idx: 0, from: PRESETS[0], blend: 1.0, idle: 0.0, track: 0, pos: 72.0, playing: true, level: 1.0 });
