//! Dynamic-Island-style pill: springy size, compact / expanded / minimal shapes for Live Activities.

use crate::{Buffer, Color, Frame, Rect, Style};

const BLACK: Color = Color::Rgb(0, 0, 0);
const TEXT: Color = Color::rgb(0xf2f2f7);
const DIM: Color = Color::rgb(0x8e8e93);
const STIFFNESS: f32 = 170.0;
const DAMPING: f32 = 22.0; // zeta ~0.85: <1% overshoot, settled in ~0.45 s
const CUTOUT: u16 = 4; // empty middle of the compact pill, like the sensor housing
const EXP_W: u16 = 52;

/// Critically-ish damped spring on one value.
#[derive(Clone, Copy, Debug, Default)]
pub struct Spring {
    pub pos: f32,
    pub vel: f32,
    pub target: f32,
}

impl Spring {
    pub fn new(v: f32) -> Spring {
        Spring { pos: v, vel: 0.0, target: v }
    }

    /// Fixed 4 ms substeps of semi-implicit Euler, so frame rate doesn't change the motion and huge `dt` can't blow up.
    pub fn step(&mut self, dt: f32) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.5) } else { 0.0 };
        let n = (dt / 0.004).ceil().max(1.0) as u32;
        let h = dt / n as f32;
        for _ in 0..n {
            self.vel += (STIFFNESS * (self.target - self.pos) - DAMPING * self.vel) * h;
            self.pos += self.vel * h;
        }
    }

    pub fn settled(&self) -> bool {
        (self.target - self.pos).abs() < 0.01 && self.vel.abs() < 0.01
    }
}

/// One Live Activity: icon + short `lead` and live `trail` when compact; `title`, `detail`, `progress` when expanded.
#[derive(Clone, Debug, Default)]
pub struct Activity {
    pub icon: char,
    pub tint: Color,
    pub lead: String,
    pub trail: String,
    pub title: String,
    pub detail: String,
    pub progress: Option<f32>,
    /// Pulses the icon.
    pub live: bool,
}

/// The morphing pill. Click `id` (or `toggle()`) expands it, click `id + 1` (the minimal pill) or `next()` rotates.
pub struct Island {
    pub expanded: bool,
    pub active: usize,
    /// Widest the expanded pill may grow (cells).
    pub max_w: u16,
    /// One-row rect inside the expanded pill left free when the activity has no `progress`, for app-drawn content.
    pub slot: Rect,
    w: Spring,
    h: Spring,
    m: Spring,
    appear: f32,
    n: usize,
    init: bool,
}

impl Default for Island {
    fn default() -> Island {
        Island { expanded: false, active: 0, max_w: EXP_W, slot: Rect::default(), w: Spring::default(), h: Spring::default(), m: Spring::default(), appear: 1.0, n: 0, init: false }
    }
}

fn len(s: &str) -> u16 {
    s.chars().count().min(u16::MAX as usize) as u16
}

/// Opaque black pill: half blocks cap a 1-row pill, quarter blocks round taller ones. Edges blend into whatever bg is below.
pub fn pill(b: &mut Buffer, r: Rect) {
    if r.w < 2 || r.h == 0 {
        return;
    }
    for y in r.y..r.bottom() {
        for x in r.x..r.right() {
            let (l, rt, t, bt) = (x == r.x, x == r.right() - 1, y == r.y, y == r.bottom() - 1);
            let ch = match (l, rt, t, bt, r.h) {
                (true, _, _, _, 1) => '▐',
                (_, true, _, _, 1) => '▌',
                (true, _, true, _, _) => '▗',
                (_, true, true, _, _) => '▖',
                (true, _, _, true, _) => '▝',
                (_, true, _, true, _) => '▘',
                _ => {
                    b.set(x, y, ' ', Style::new().bg(BLACK));
                    continue;
                }
            };
            let under = b.get(x, y).bg;
            b.set(x, y, ch, Style::new().fg(BLACK).bg(under));
        }
    }
}

impl Island {
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    /// Rotates to the next activity (count as of the last `draw`).
    pub fn next(&mut self) {
        if self.n > 1 {
            self.active = (self.active + 1) % self.n;
            self.appear = 0.0;
        }
    }

    /// Steps the springs and draws centred at the top of `area`. Returns the main pill's rect (empty if `area` is too small).
    pub fn draw(&mut self, f: &mut Frame, area: Rect, acts: &[Activity], dt: f32, id: u32) -> Rect {
        self.slot = Rect::default();
        self.n = acts.len();
        if acts.is_empty() || area.w < 8 || area.h < 1 {
            return Rect::new(area.x, area.y, 0, 0);
        }
        self.active %= acts.len();
        let a = &acts[self.active];
        let (tl, tt) = (len(&a.lead), len(&a.trail));

        // Targets. Compact is symmetric around the cutout so lead and trail balance.
        let side = (if tl > 0 { 2 + tl } else { 1 }).max(tt);
        let eh = area.h.min(5);
        let ew = (6 + len(&a.title) + 2 + tt).max(len(&a.detail) + 6).clamp(30, self.max_w.max(30));
        let (tw, th) = if self.expanded { (ew, eh) } else { (2 * (side + 2) + CUTOUT, 1) };
        let minimal = acts.len() > 1 && !self.expanded;
        let nx = &acts[(self.active + 1) % acts.len()];
        let mw = 5 + if nx.trail.is_empty() { 0 } else { 1 + len(&nx.trail).min(5) };
        self.w.target = tw.min(area.w) as f32;
        self.h.target = th as f32;
        self.m.target = if minimal { mw as f32 } else { 0.0 };
        if !self.init {
            (self.w.pos, self.h.pos, self.init) = (self.w.target, self.h.target, true);
        }
        let dt = dt.max(0.0);
        for s in [&mut self.w, &mut self.h, &mut self.m] {
            s.step(dt);
        }
        self.appear = (self.appear + dt * 5.0).min(1.0);

        // Geometry: the group (pill + gap + minimal) is centred as one so adding a neighbour slides the main pill over.
        let pw = (self.w.pos.round().max(3.0) as u16).min(area.w);
        let ph = (self.h.pos.round().max(1.0) as u16).min(area.h);
        let mrect_w = if self.m.pos >= 2.5 { (self.m.pos.round() as u16).min(area.w) } else { 0 };
        let total = (pw + if mrect_w > 0 { 1 + mrect_w } else { 0 }).min(area.w);
        let r = Rect::new(area.x + (area.w - total) / 2, area.y, pw.min(total), ph);
        pill(f.buf, r);
        f.hit(r, id);

        // Content fades in as the size settles; `p` is expansion progress (0 compact .. 1 full).
        let err = (self.w.pos - self.w.target).abs() / (self.w.target.max(8.0) * 0.5) + (self.h.pos - self.h.target).abs() / (eh.max(2) as f32 * 0.5);
        let g = (1.0 - err * 0.7).clamp(0.0, 1.0) * self.appear;
        let p = if eh > 1 { ((self.h.pos - 1.0) / (eh - 1) as f32).clamp(0.0, 1.0) } else { 0.0 };
        let lead_a = (1.0 - p * 2.5).clamp(0.0, 1.0);
        let body_a = ((p - 0.35) / 0.65).clamp(0.0, 1.0);
        let off = u16::from(eh >= 5);
        let (x0, x1) = (r.x + 2, r.right().saturating_sub(2));
        let row = |y: u16, x: u16| if y < r.bottom() && x < x1 { Some(y) } else { None };
        let put = |b: &mut Buffer, x: u16, y: u16, s: &str, c: Color, al: f32, bold: bool| {
            if al * g < 0.06 || row(y, x).is_none() {
                return;
            }
            let st = Style::new().fg(BLACK.mix(c, al * g)).bg(BLACK);
            b.print(x, y, s, if bold { st.bold() } else { st }, x1 - x);
        };
        if x1 > x0 {
            let ty = r.y + (p * off as f32).round() as u16;
            let pulse = if a.live { 0.2 * (f.time as f32 * 4.0).sin() } else { 0.0 };
            let icon = a.tint.mix(if pulse > 0.0 { TEXT } else { BLACK }, pulse.abs());
            put(f.buf, x0, ty, &a.icon.to_string(), icon, 1.0, true);
            let room = (x1 - x0).saturating_sub(3 + tt);
            let tx = x1.saturating_sub(tt).max(x0);
            put(f.buf, tx, ty, &a.trail, a.tint, 1.0, true);
            let lead: String = a.lead.chars().take(room as usize).collect();
            put(f.buf, x0 + 2, r.y, &lead, TEXT, lead_a, false);
            let title: String = a.title.chars().take(room as usize).collect();
            put(f.buf, x0 + 2, ty, &title, TEXT, body_a, true);
            let detail: String = a.detail.chars().take((x1 - x0).saturating_sub(2) as usize).collect();
            put(f.buf, x0 + 2, r.y + off + 1, &detail, DIM, body_a, false);
            let ry = r.y + off + 2;
            let rw = x1 - x0;
            if ry < r.bottom() && p > 0.95 && a.progress.is_none() {
                self.slot = Rect::new(x0, ry, rw, 1);
            } else if let (Some(pr), true) = (a.progress, ry < r.bottom()) {
                let k = (pr.clamp(0.0, 1.0) * rw as f32).round() as u16;
                for i in 0..rw {
                    let (ch, c) = if i < k { ('━', a.tint) } else { ('─', a.tint.mix(BLACK, 0.78)) };
                    f.buf.set(x0 + i, ry, ch, Style::new().fg(BLACK.mix(c, body_a * g)).bg(BLACK));
                }
            }
        }

        // Minimal: the next activity as a detached pill, icon + tiny value.
        if mrect_w > 0 && total > pw + 1 {
            let mr = Rect::new(r.x + pw + 1, r.y, (total - pw - 1).min(mrect_w), 1);
            pill(f.buf, mr);
            f.hit(mr, id + 1);
            let al = (self.m.pos / mw as f32).clamp(0.0, 1.0).powi(2);
            let mx1 = mr.right().saturating_sub(2);
            let st = |c: Color| Style::new().fg(BLACK.mix(c, al)).bg(BLACK);
            if mr.x + 2 < mx1 {
                f.buf.set(mr.x + 2, mr.y, nx.icon, st(nx.tint).bold());
                let v: String = nx.trail.chars().take(5).collect();
                f.buf.print(mr.x + 4, mr.y, &v, st(nx.tint), mx1.saturating_sub(mr.x + 4));
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Runtime;

    fn acts() -> Vec<Activity> {
        let mk = |lead: &str, trail: &str| Activity { icon: '♪', tint: Color::rgb(0xff9f0a), lead: lead.into(), trail: trail.into(), title: "Title".into(), detail: "detail".into(), progress: Some(0.4), live: true };
        vec![mk("Song", "1:23"), mk("Build", "42%")]
    }

    struct Probe(Island, bool, Rect);
    impl crate::App for Probe {
        fn view(&mut self, f: &mut Frame) {
            self.0.expanded = self.1;
            let a = f.area;
            self.2 = self.0.draw(f, a, &acts(), 0.016, 1);
        }
    }

    fn run(w: u16, h: u16, expanded: bool) -> Rect {
        let mut rt = Runtime::new(Box::new(Probe(Island::default(), expanded, Rect::default())), w, h);
        for _ in 0..60 {
            rt.tick(0.016);
            rt.render();
        }
        // last frame's pill must stay inside the buffer
        let mut p = Probe(Island::default(), expanded, Rect::default());
        let mut hits = vec![];
        let mut buf = Buffer::new(w, h);
        let mut f = Frame { buf: &mut buf, area: Rect::new(0, 0, w, h), hits: &mut hits, mouse: None, time: 0.0 };
        crate::App::view(&mut p, &mut f);
        p.2
    }

    #[test]
    fn spring_converges_without_nan() {
        let mut s = Spring { pos: 0.0, vel: 0.0, target: 50.0 };
        for _ in 0..37 {
            s.step(0.016);
        }
        assert!((s.pos - 50.0).abs() < 0.5, "{}", s.pos);
        s.step(1e9);
        s.step(f32::NAN);
        assert!(s.pos.is_finite() && s.vel.is_finite());
    }

    #[test]
    fn draws_small_and_large_without_panic() {
        for (w, h) in [(10, 1), (200, 8), (3, 1), (8, 0), (20, 2)] {
            for e in [false, true] {
                let r = run(w, h, e);
                assert!(r.right() <= w.max(r.x) && r.bottom() <= h.max(r.y));
            }
        }
    }

    #[test]
    fn compact_width_fits_content() {
        let r = run(120, 8, false);
        // icon + gap + "Song" and "1:23", both sides padded, plus the cutout
        assert!(r.w >= 2 + 2 + 4 + CUTOUT + 4 + 2 && r.w < 40, "{}", r.w);
        assert_eq!(r.h, 1);
    }
}
