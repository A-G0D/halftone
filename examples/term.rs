//! `cargo run --release --example term [-- cmd args...]`: a real shell (or any TUI) inside a halftone
//! pane, recoloured live to the current theme. ctrl+q quits, wheel scrolls history.

use halftone::prelude::*;
use halftone::pty::Pty;
use halftone::vt::{key_bytes, Palette, Vt};

struct Term {
    pty: Pty,
    vt: Vt,
}

impl Default for Term {
    fn default() -> Term {
        let mut a: Vec<String> = std::env::args().skip(1).collect();
        if a.is_empty() {
            a.push(std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()));
        }
        let rest: Vec<&str> = a[1..].iter().map(String::as_str).collect();
        let (w, h) = halftone::term::size();
        let pty = Pty::spawn(&a[0], &rest, None, w, h.saturating_sub(1).max(1)).unwrap_or_else(|e| {
            eprintln!("term: {e}");
            std::process::exit(1)
        });
        Term { pty, vt: Vt::new(w, h.saturating_sub(1).max(1)) }
    }
}

impl App for Term {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match ev {
            Event::Key(Key { code: KeyCode::Char('q'), ctrl: true, .. }) => ctx.quit(),
            Event::Key(k) => {
                self.vt.scroll = 0;
                self.pty.write(&key_bytes(k, self.vt.app_cursor));
            }
            Event::Mouse { kind: MouseKind::ScrollUp, .. } => self.vt.scroll += 3,
            Event::Mouse { kind: MouseKind::ScrollDown, .. } => self.vt.scroll = self.vt.scroll.saturating_sub(3),
            Event::Tick(_) => {
                self.vt.feed(&self.pty.try_read());
                let r = self.vt.take_replies();
                if !r.is_empty() {
                    self.pty.write(&r);
                }
                if !self.pty.alive() {
                    ctx.quit();
                }
            }
            _ => {}
        }
    }

    fn view(&mut self, f: &mut Frame) {
        let rows = f.area.rows(&[Fill(1), Len(1)], 0);
        let (w, h) = (rows[0].w.max(1), rows[0].h.max(1));
        if self.vt.size() != (w, h) {
            self.vt.resize(w, h);
            self.pty.resize(w, h);
        }
        self.vt.draw(f.buf, rows[0], &Palette::from_theme());
        f.buf.fill(rows[1], theme::panel());
        f.buf.print(rows[1].x + 1, rows[1].y, &format!("ctrl+q quit · {}", self.vt.title), fg(theme::muted()).bg(theme::panel()), rows[1].w);
    }
}

halftone::main!(Term::default());
