//! `halftone wrap <command>`: runs a CLI (Claude Code, codex, anything) full screen as if in a plain
//! terminal, re-themed at the cell level, with halftone's mods on top: voice, shortcuts, toasts.
//! It reads only what the program prints, so it keeps working when the program updates.

use super::config;
use super::term::TermPane;
use super::voice::Voice;
use crate::toast::Toasts;
use crate::{fg, theme, App, Ctx, Event, Frame, KeyCode, Rect};

pub struct Wrap {
    pane: TermPane,
    keys: Vec<(String, String)>,
    voice: Option<Voice>,
    toasts: Toasts,
    note: Option<(String, f64)>,
    t: f64,
    dt: f32,
    seen: u64,
}

impl Wrap {
    pub fn new(cmd: &str) -> Wrap {
        crate::term::keep_ctrl_c(true);
        let cfg = config::load().ok();
        if let Some(c) = &cfg {
            theme::set(theme::PRESETS[super::theme_index(&c.theme)]);
        }
        let seen = std::fs::metadata(super::inbox()).map(|m| m.len()).unwrap_or(0);
        Wrap { pane: TermPane::new(cmd), keys: cfg.map(|c| c.keys).unwrap_or_default(), voice: None, toasts: Toasts::new(), note: None, t: 0.0, dt: 0.016, seen }
    }
}

impl App for Wrap {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match ev {
            Event::Tick(dt) => {
                self.t += dt;
                self.dt = *dt as f32;
                self.pane.pump(*dt);
                if self.pane.exited {
                    ctx.quit();
                }
                let p = super::inbox();
                if let Ok(m) = std::fs::metadata(&p) {
                    if m.len() > self.seen {
                        if let Ok(b) = std::fs::read(&p) {
                            for line in String::from_utf8_lossy(&b[self.seen as usize..]).lines() {
                                let mut it = line.splitn(3, '\t');
                                let (a, ti, bo) = (it.next().unwrap_or(""), it.next().unwrap_or(""), it.next().unwrap_or(""));
                                self.toasts.push(self.t, a, super::cards::tone(super::app_tint(a)), ti, bo);
                            }
                        }
                    }
                    self.seen = m.len();
                }
            }
            Event::Key(k) => {
                let voice_key = self.keys.iter().any(|(c, a)| a == "voice" && super::combo_matches(c, k));
                if voice_key || k.ctrl && k.code == KeyCode::Char(' ') {
                    match self.voice.take() {
                        None => match Voice::start() {
                            Ok(v) => self.voice = Some(v),
                            Err(e) => self.note = Some((e, self.t)),
                        },
                        Some(v) => match v.finish() {
                            Ok(text) if !text.is_empty() => self.pane.paste(&text),
                            Ok(_) => self.note = Some(("heard nothing".into(), self.t)),
                            Err(e) => self.note = Some((e, self.t)),
                        },
                    }
                    return;
                }
                for (combo, action) in &self.keys {
                    if super::combo_matches(combo, k) {
                        if let Some(text) = action.strip_prefix("send ") {
                            self.pane.send(super::unescape(text).as_bytes());
                            return;
                        }
                    }
                }
                self.pane.key(k);
            }
            _ => {}
        }
    }

    fn view(&mut self, f: &mut Frame) {
        let a = f.area;
        self.pane.draw(f.buf, a);
        let dt = self.dt;
        self.toasts.draw(f, Rect::new(a.x, a.y + 1, a.w, a.h.saturating_sub(2)), dt, 1);
        let pill = |f: &mut Frame, text: &str, dot: crate::Color| {
            let w = (text.chars().count() as u16 + 6).min(a.w);
            let r = Rect::new(a.x + (a.w - w) / 2, a.bottom().saturating_sub(2), w, 1);
            f.buf.fill(r, theme::raised());
            f.buf.set(r.x + 2, r.y, '●', fg(dot));
            f.buf.print(r.x + 4, r.y, text, fg(theme::text()), w - 5);
        };
        if let Some(v) = &self.voice {
            let lv = v.levels();
            let wave: String = lv.iter().map(|l| ['⠀', '⣀', '⣤', '⣶', '⣿'][((l * 4.0).round() as usize).min(4)]).collect();
            let pulse = 0.5 + 0.5 * (self.t * 4.0).sin() as f32;
            pill(f, &format!("Listening  {wave}  press again to send"), theme::bad().mix(theme::raised(), pulse * 0.5));
        } else if let Some((msg, at)) = &self.note {
            if self.t - at < 4.0 {
                pill(f, msg, theme::muted());
            }
        }
    }
}
