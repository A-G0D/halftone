//! A program in a pty, drawn through the theme palette, so any TUI (Claude Code, codex, vim,
//! lazygit, a shell) takes the workspace colours at the cell level, whatever version it is.

use crate::pty::Pty;
use crate::vt::{key_bytes, Palette, Vt};
use crate::{Buffer, Key, Rect};

pub struct TermPane {
    pub cmd: String,
    pty: Option<Pty>,
    pub vt: Vt,
    size: (u16, u16),
    pub exited: bool,
    pub err: Option<String>,
    /// Seconds since the program last printed, for activity dots.
    pub quiet: f64,
}

impl TermPane {
    pub fn new(cmd: &str) -> TermPane {
        TermPane { cmd: cmd.into(), pty: None, vt: Vt::new(80, 24), size: (0, 0), exited: false, err: None, quiet: 0.0 }
    }

    fn spawn(&mut self, cols: u16, rows: u16) {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let cwd = std::env::var("HALFTONE_TERM_CWD").ok();
        // A login shell runs the command so PATH matches what the user types themselves.
        let res = if self.cmd.is_empty() || self.cmd == "shell" {
            Pty::spawn(&shell, &["-l"], cwd.as_deref(), cols, rows)
        } else {
            Pty::spawn(&shell, &["-lc", &self.cmd], cwd.as_deref(), cols, rows)
        };
        match res {
            Ok(p) => self.pty = Some(p),
            Err(e) => self.err = Some(e),
        }
    }

    /// Moves output into the screen and answers the program's terminal queries.
    pub fn pump(&mut self, dt: f64) {
        let Some(p) = &mut self.pty else { return };
        let bytes = p.try_read();
        if bytes.is_empty() {
            self.quiet += dt;
        } else {
            self.quiet = 0.0;
            self.vt.feed(&bytes);
            let r = self.vt.take_replies();
            if !r.is_empty() {
                p.write(&r);
            }
        }
        if !p.alive() {
            self.exited = true;
        }
    }

    pub fn draw(&mut self, b: &mut Buffer, r: Rect) {
        if r.w < 4 || r.h < 2 {
            return;
        }
        if self.pty.is_none() && self.err.is_none() && !self.exited {
            self.vt.resize(r.w, r.h);
            self.spawn(r.w, r.h);
            self.size = (r.w, r.h);
        } else if self.size != (r.w, r.h) {
            self.size = (r.w, r.h);
            self.vt.resize(r.w, r.h);
            if let Some(p) = &mut self.pty {
                p.resize(r.w, r.h);
            }
        }
        b.fill(r, crate::theme::bg());
        self.vt.draw(b, r, &Palette::from_theme());
        if let Some(e) = &self.err {
            b.print(r.x, r.y, e, crate::fg(crate::theme::bad()), r.w);
        }
    }

    pub fn key(&mut self, k: &Key) {
        let bytes = key_bytes(k, self.vt.app_cursor);
        self.send(&bytes);
    }

    pub fn send(&mut self, bytes: &[u8]) {
        if let Some(p) = &mut self.pty {
            p.write(bytes);
        }
    }

    /// Text typed as if pasted, so a CLI does not treat newlines inside it as Enter.
    pub fn paste(&mut self, text: &str) {
        let s = if self.vt.bracketed_paste { format!("\x1b[200~{text}\x1b[201~") } else { text.to_string() };
        self.send(s.as_bytes());
    }
}
