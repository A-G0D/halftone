//! A dithered planet lit by your cursor. `cargo run --release --example demo`.
//! `t` or click changes theme, `q` quits.

use halftone::prelude::*;
use halftone::theme::{Theme, PRESETS};

/// 8×8 ordered-dither threshold in 0..1.
fn bayer(x: i32, y: i32) -> f32 {
    let mut v = 0;
    for i in 0..3 {
        let (xb, yb) = ((x >> i) & 1, (y >> i) & 1);
        v = (v << 2) | ((xb ^ yb) << 1) | yb;
    }
    (v as f32 + 0.5) / 64.0
}

fn norm((x, y, z): (f32, f32, f32)) -> (f32, f32, f32) {
    let l = (x * x + y * y + z * z).sqrt().max(1e-6);
    (x / l, y / l, z / l)
}

struct Orb {
    light: (f32, f32),
    idx: usize,
    from: Theme,
    blend: f32,
    shown: f32,
}

impl Orb {
    fn next_theme(&mut self) {
        self.from = self.current();
        self.idx = (self.idx + 1) % PRESETS.len();
        (self.blend, self.shown) = (0.0, 0.0);
    }
    fn current(&self) -> Theme {
        let t = self.blend.clamp(0.0, 1.0);
        self.from.mix(&PRESETS[self.idx], t * t * (3.0 - 2.0 * t))
    }
}

impl App for Orb {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match *ev {
            Event::Tick(dt) => {
                self.blend += dt as f32 / 0.6;
                self.shown += dt as f32;
                theme::set(self.current());
            }
            Event::Mouse { kind: MouseKind::Down, .. } => self.next_theme(),
            _ => match ev.ch() {
                Some('t') => self.next_theme(),
                Some('q') => ctx.quit(),
                _ => {}
            },
        }
    }

    fn view(&mut self, f: &mut Frame) {
        let a = f.area;
        if a.w < 8 || a.h < 4 {
            return;
        }
        let stage = Rect::new(a.x, a.y, a.w, a.h - 1);
        let (w, h) = (stage.w as i32 * 2, stage.h as i32 * 4);
        let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
        let r = (w as f32 / 2.0 / 1.75).min(h as f32 / 2.0 * 0.82);

        // Light follows the cursor; otherwise it drifts around the planet.
        let t = f.time as f32;
        let target = match f.mouse {
            Some((mx, my)) => ((((mx - stage.x) as f32 * 2.0 - cx) / r).clamp(-1.6, 1.6), (((my - stage.y) as f32 * 4.0 - cy) / r).clamp(-1.6, 1.6)),
            None => ((t * 0.35).cos() * 1.1, (t * 0.35).sin() * 0.45 - 0.35),
        };
        self.light.0 += (target.0 - self.light.0) * 0.08;
        self.light.1 += (target.1 - self.light.1) * 0.08;
        let l = norm((self.light.0, self.light.1, 0.75));
        let hv = norm((l.0, l.1, l.2 + 1.0));

        let (dim, lit, hot, ring) = (theme::faint(), theme::accent(), theme::text(), theme::muted());
        let mut c = Canvas::new(stage.w, stage.h);
        let (ry, tilt) = (0.22, -0.18f32);
        let (st, ct) = tilt.sin_cos();
        // Ring dots: behind the planet first, then the planet, then the front half on top.
        let ring_pts = |front: bool, c: &mut Canvas| {
            for (k, rad) in [(0usize, 1.45f32), (1, 1.62)] {
                let n = (rad * r * 7.0) as i32;
                for i in 0..n {
                    let th = i as f32 / n as f32 * std::f32::consts::TAU;
                    if (th.sin() > 0.0) != front || (i + k as i32 * 2 + (t * 6.0) as i32) % 4 == 0 {
                        continue;
                    }
                    let (ex, ey) = (th.cos() * rad * r, th.sin() * rad * r * ry);
                    c.dot((cx + ex * ct - ey * st) as i32, (cy + ex * st + ey * ct) as i32, ring);
                }
            }
        };
        ring_pts(false, &mut c);
        for py in (cy - r) as i32..=(cy + r) as i32 {
            for px in (cx - r) as i32..=(cx + r) as i32 {
                let (dx, dy) = ((px as f32 + 0.5 - cx) / r, (py as f32 + 0.5 - cy) / r);
                let d2 = dx * dx + dy * dy;
                if d2 > 1.0 {
                    continue;
                }
                let n = (dx, dy, (1.0 - d2).sqrt());
                let diff = (n.0 * l.0 + n.1 * l.1 + n.2 * l.2).max(0.0);
                let spec = (n.0 * hv.0 + n.1 * hv.1 + n.2 * hv.2).max(0.0).powi(28);
                let i = (0.04 + diff * 0.9 + spec * 0.6).min(1.0);
                if i > bayer(px, py) {
                    c.dot(px, py, if spec > 0.35 { lit.mix(hot, spec) } else { dim.mix(lit, i) });
                }
            }
        }
        ring_pts(true, &mut c);
        c.draw(f.buf, stage.x, stage.y);

        let y = a.bottom() - 1;
        f.buf.print(a.x + 2, y, "halftone", fg(theme::muted()), a.w);
        let hint = "t theme";
        f.buf.print(a.right().saturating_sub(hint.len() as u16 + 2), y, hint, fg(theme::faint()), a.w);
        // Theme name fades in, holds, then fades out.
        let o = (self.shown / 0.3).min(1.0).min(((2.2 - self.shown) / 0.5).max(0.0));
        if o > 0.0 {
            let name = PRESETS[self.idx].name;
            let x = a.x + (a.w.saturating_sub(name.len() as u16)) / 2;
            f.buf.print(x, y, name, fg(theme::bg().mix(theme::accent(), o)), a.w);
        }
    }
}

halftone::main!(Orb { light: (0.8, -0.4), idx: 0, from: PRESETS[0], blend: 1.0, shown: 9.0 });
