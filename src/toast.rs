//! Notification toasts, the way chat and mail apps do them: app, sender, one-line preview,
//! stacked in a corner, sliding in, settling up as older ones leave, grouped past three.

use crate::motion::{ease_out, Tween};
use crate::{fg, theme, Color, Frame, Rect, Style};

pub struct Toast {
    pub app: String,
    /// Brand colour for the app chip and the edge stripe.
    pub tint: Color,
    pub title: String,
    pub body: String,
    born: f64,
    y: Tween,
}

#[derive(Default)]
pub struct Toasts {
    pub list: Vec<Toast>,
    /// Seconds a toast stays before it leaves.
    pub life: f64,
}

const W: u16 = 52;
const H: u16 = 5;
const SHOWN: usize = 3;

impl Toasts {
    pub fn new() -> Toasts {
        Toasts { list: vec![], life: 6.0 }
    }

    pub fn push(&mut self, now: f64, app: &str, tint: Color, title: &str, body: &str) {
        let mut y = Tween::new(-1.0, 0.35);
        y.snap(0.0);
        self.list.insert(0, Toast { app: app.into(), tint, title: title.into(), body: body.into(), born: now, y });
    }

    /// Hit ids are `id_base + index`. Returns how many are on screen.
    pub fn draw(&mut self, f: &mut Frame, area: Rect, dt: f32, id_base: u32) -> usize {
        let now = f.time;
        let life = self.life;
        self.list.retain(|t| now - t.born < life);
        let extra = self.list.len().saturating_sub(SHOWN);
        let w = W.min(area.w.saturating_sub(2));
        for (i, t) in self.list.iter_mut().take(SHOWN).enumerate() {
            t.y.set(i as f32);
            t.y.step(dt);
            let age = now - t.born;
            let enter = ease_out((age / 0.35) as f32);
            let leave = ((age - (life - 0.4)) / 0.4).clamp(0.0, 1.0) as f32;
            let dx = ((1.0 - enter) * (w as f32 + 2.0)) as u16;
            let x = area.right().saturating_sub(w + 1) + dx;
            let y = area.y + (t.y.get() * (H + 1) as f32).round() as u16;
            if y + H > area.bottom() || x >= area.right() {
                continue;
            }
            let ww = w.min(area.right() - x);
            let r = Rect::new(x, y, ww, H);
            let fade = leave.max(1.0 - enter);
            let bg = theme::raised();
            let c = |col: Color| col.mix(theme::bg(), fade);
            f.buf.fill(r, c(bg));
            for k in 0..H {
                f.buf.set(x, y + k, '▎', fg(c(t.tint)));
            }
            let chip = Style::new().fg(c(theme::bg())).bg(c(t.tint)).bold();
            let initial = t.app.chars().next().unwrap_or('•').to_ascii_uppercase();
            f.buf.print(x + 2, y + 1, &format!(" {initial} "), chip, 3);
            let tx = x + 6;
            let tw = ww.saturating_sub(8);
            let app_w = f.buf.print(tx, y + 1, &t.app, fg(c(theme::muted())), tw);
            let ago = if age < 60.0 { "now".to_string() } else { format!("{}m", (age / 60.0) as u32) };
            f.buf.print(app_w + 1, y + 1, &format!("· {ago}"), fg(c(theme::faint())), tw);
            crate::workspace::cards::fit(f.buf, tx, y + 2, tw, &t.title, fg(c(theme::text())).bold());
            if extra > 0 && i + 1 == SHOWN {
                f.buf.print(tx, y + 3, &format!("+{extra} more"), fg(c(theme::accent())), tw);
            } else {
                crate::workspace::cards::fit(f.buf, tx, y + 3, tw, &t.body, fg(c(theme::muted())));
            }
            f.hit(r, id_base + i as u32);
        }
        self.list.len().min(SHOWN)
    }

    pub fn dismiss(&mut self, i: usize) {
        if i < self.list.len() {
            self.list.remove(i);
        }
    }
}
