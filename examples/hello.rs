//! The smallest Halftone app: `cargo run --example hello`.

use halftone::prelude::*;

#[derive(Default)]
struct Hello {
    clicks: u32,
}

impl App for Hello {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match ev {
            Event::Click { id: 1, .. } => self.clicks += 1,
            e if e.ch() == Some(' ') => self.clicks += 1,
            e if e.ch() == Some('q') => ctx.quit(),
            _ => {}
        }
    }

    fn view(&mut self, f: &mut Frame) {
        let r = Rect::new(f.area.w.saturating_sub(36) / 2, f.area.h.saturating_sub(7) / 2, 36, 7);
        let inner = panel(f.buf, r, "hello", true);
        f.buf.print(inner.x, inner.y, &format!("clicked {} times", self.clicks), fg(theme::text()), inner.w);
        button(f, Rect::new(inner.x, inner.y + 2, 12, 1), "click", 1, false);
        keys(f.buf, inner.x, inner.y + 4, &[("space", "click"), ("q", "quit")]);
    }
}

halftone::main!(Hello::default());
