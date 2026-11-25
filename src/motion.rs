//! Small motion kit: eased tweens, fades and offset blits for slide/pop transitions.

use crate::{Buffer, Color, Rect};

/// Strong ease-out (cubic-bezier 0.23, 1, 0.32, 1 is close to quint out).
pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(5)
}

pub fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 { 16.0 * t.powi(5) } else { 1.0 - (-2.0 * t + 2.0).powi(5) / 2.0 }
}

/// A value that eases from where it is to wherever it was last sent, over `dur` seconds.
#[derive(Clone, Copy, Debug)]
pub struct Tween {
    from: f32,
    to: f32,
    t: f32,
    dur: f32,
}

impl Tween {
    pub fn new(v: f32, dur: f32) -> Tween {
        Tween { from: v, to: v, t: 1.0, dur }
    }
    /// Retargets from the current value, so interrupting mid-flight never jumps.
    pub fn set(&mut self, v: f32) {
        if v != self.to {
            (self.from, self.to, self.t) = (self.get(), v, 0.0);
        }
    }
    /// Jumps without animating.
    pub fn snap(&mut self, v: f32) {
        (self.from, self.to, self.t) = (v, v, 1.0);
    }
    pub fn step(&mut self, dt: f32) {
        self.t = (self.t + dt / self.dur.max(1e-3)).min(1.0);
    }
    pub fn get(&self) -> f32 {
        self.from + (self.to - self.from) * ease_out(self.t)
    }
    pub fn target(&self) -> f32 {
        self.to
    }
    pub fn done(&self) -> bool {
        self.t >= 1.0
    }
}

/// 0→1 over `dur` seconds since `at`, eased out. Handy for one-shot entrances.
pub fn since(now: f64, at: f64, dur: f64) -> f32 {
    ease_out(((now - at) / dur) as f32)
}

/// Copies `src` into `dst` shifted by `dx` columns, clipped to `clip`, with every colour pulled
/// toward `bg` by `fade` (0 = untouched). Drives slide and cross-fade transitions.
pub fn blit(dst: &mut Buffer, src: &Buffer, clip: Rect, dx: i32, fade: f32, bg: Color) {
    for y in clip.y..clip.bottom().min(src.h).min(dst.h) {
        for x in clip.x..clip.right().min(dst.w) {
            let sx = x as i32 - dx;
            if sx < clip.x as i32 || sx >= clip.right() as i32 || sx >= src.w as i32 {
                continue;
            }
            let mut c = src.get(sx as u16, y);
            if fade > 0.0 {
                c.fg = c.fg.mix(bg, fade);
                c.bg = c.bg.mix(bg, fade);
            }
            if let Some(d) = dst.cell(x, y) {
                *d = c;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tween_retargets_without_jumping() {
        let mut t = Tween::new(0.0, 1.0);
        t.set(10.0);
        t.step(0.5);
        let mid = t.get();
        assert!(mid > 5.0 && mid < 10.0);
        t.set(0.0);
        assert_eq!(t.get(), mid);
        t.step(1.0);
        assert_eq!(t.get(), 0.0);
    }
}
