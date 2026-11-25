//! One photo, four ways to draw it. `cargo run --release --example image`.
//! Click a mode or press space; `q` quits.

use halftone::prelude::*;
use halftone::image;

const MODES: [(Mode, &str); 4] = [(Mode::Half, "half"), (Mode::Braille, "braille"), (Mode::Blocks, "blocks"), (Mode::Ascii, "ascii")];
const PICK: u32 = 10;

struct Lab {
    img: Image,
    cur: usize,
    prev: usize,
    sweep: f32,
    idle: f32,
}

impl Lab {
    fn go(&mut self, i: usize) {
        if i != self.cur {
            (self.prev, self.cur, self.sweep) = (self.cur, i % MODES.len(), 0.0);
        }
        self.idle = 0.0;
    }
}

impl App for Lab {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match *ev {
            Event::Tick(dt) => {
                self.sweep += dt as f32 / 0.7;
                self.idle += dt as f32;
                if self.idle > 2.6 {
                    self.go(self.cur + 1);
                }
            }
            Event::Click { id, .. } if (PICK..PICK + 4).contains(&id) => self.go((id - PICK) as usize),
            _ => match ev.ch() {
                Some(' ') => self.go(self.cur + 1),
                Some('q') => ctx.quit(),
                _ => {}
            },
        }
    }

    fn view(&mut self, f: &mut Frame) {
        let a = f.area;
        if a.w < 20 || a.h < 6 {
            return;
        }
        let stage = Rect::new(a.x, a.y, a.w, a.h - 2);
        let opts = |i: usize| Opts { mode: MODES[i].0, ..Opts::default() };
        let s = self.sweep.clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - s).powi(3);
        let sx = stage.x + (stage.w as f32 * e) as u16;

        // Old mode stays right of the sweep line, new mode fills in behind it.
        let mut keep = Vec::new();
        if s < 1.0 {
            image::draw(f.buf, stage, &self.img, opts(self.prev));
            for y in stage.y..stage.bottom() {
                for x in sx..stage.right() {
                    keep.push((x, y, f.buf.get(x, y)));
                }
            }
            f.buf.fill(stage, theme::bg());
        }
        image::draw(f.buf, stage, &self.img, opts(self.cur));
        for (x, y, c) in keep {
            if let Some(cell) = f.buf.cell(x, y) {
                *cell = c;
            }
        }
        if s < 1.0 && sx < stage.right() {
            for y in stage.y..stage.bottom() {
                f.buf.set(sx, y, '▏', fg(theme::accent()));
            }
        }

        // Mode picker: the current one bright, the rest quiet.
        let y = a.bottom() - 1;
        let total: u16 = MODES.iter().map(|m| m.1.len() as u16).sum::<u16>() + 3 * 3;
        let mut x = a.x + a.w.saturating_sub(total) / 2;
        for (i, (_, name)) in MODES.iter().enumerate() {
            let r = Rect::new(x, y, name.len() as u16, 1);
            let c = if i == self.cur { theme::text() } else if f.hover(r) { theme::muted() } else { theme::faint() };
            x = f.buf.print(x, y, name, fg(c), a.w);
            f.hit(r, PICK + i as u32);
            if i + 1 < MODES.len() {
                x = f.buf.print(x, y, " · ", fg(theme::faint()), a.w);
            }
        }
    }
}

/// Earthrise with most of the empty sky cropped off, so Earth and the lunar horizon fill the frame.
fn earthrise() -> Image {
    let img = Image::decode(include_bytes!("../assets/earthrise.bmp")).expect("bundled image decodes");
    let top = img.h * 42 / 100; // leaves a ~1.7:1 frame, the shape of the stage
    Image { w: img.w, h: img.h - top, px: img.px[(top * img.w) as usize..].to_vec() }
}

halftone::main!(Lab {
    img: earthrise(),
    cur: 0,
    prev: 0,
    sweep: 1.0,
    idle: 0.0,
});
