//! Child process on a pseudo-terminal: `forkpty`, a reader thread feeding a channel, resize, kill on drop.

use std::ffi::{c_char, c_int, c_ulong, c_void, CString};
use std::fs::File;
use std::io::Read;
use std::os::fd::FromRawFd;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{self, Receiver};

#[cfg_attr(all(target_os = "linux", not(target_env = "musl")), link(name = "util"))] // musl has openpty in libc
extern "C" {
    fn forkpty(master: *mut c_int, name: *mut c_char, term: *const c_void, ws: *const Winsize) -> c_int;
    fn execvp(file: *const c_char, argv: *const *const c_char) -> c_int;
    fn chdir(path: *const c_char) -> c_int;
    fn setenv(k: *const c_char, v: *const c_char, overwrite: c_int) -> c_int;
    fn _exit(code: c_int) -> !;
    fn waitpid(pid: c_int, status: *mut c_int, opts: c_int) -> c_int;
    fn kill(pid: c_int, sig: c_int) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn dup(fd: c_int) -> c_int;
    fn write(fd: c_int, buf: *const c_void, n: usize) -> isize;
    fn ioctl(fd: i32, req: c_ulong, ...) -> i32;
}

#[cfg(target_os = "macos")]
const TIOCSWINSZ: c_ulong = 0x8008_7467;
#[cfg(not(target_os = "macos"))]
const TIOCSWINSZ: c_ulong = 0x5414;
const WNOHANG: c_int = 1;
const SIGKILL: c_int = 9;

#[repr(C)]
struct Winsize {
    row: u16,
    col: u16,
    xpx: u16,
    ypx: u16,
}

pub struct Pty {
    fd: c_int,
    pid: c_int,
    rx: Receiver<Vec<u8>>,
    reaped: bool,
}

impl Pty {
    /// Runs `cmd args` (looked up on PATH) in a new pty of `cols`x`rows` with TERM=xterm-256color.
    pub fn spawn(cmd: &str, args: &[&str], cwd: Option<&str>, cols: u16, rows: u16) -> Result<Pty, String> {
        let c = |s: &str| CString::new(s).map_err(|e| e.to_string());
        let prog = c(cmd)?;
        let mut owned = vec![prog.clone()];
        for a in args {
            owned.push(c(a)?);
        }
        let mut argv: Vec<*const c_char> = owned.iter().map(|a| a.as_ptr()).collect();
        argv.push(null());
        let cwd = cwd.map(c).transpose()?;
        let (term, xt, ct, tc) = (c("TERM")?, c("xterm-256color")?, c("COLORTERM")?, c("truecolor")?);
        let ws = Winsize { row: rows, col: cols, xpx: 0, ypx: 0 };
        let mut fd: c_int = -1;
        // SAFETY: the child only calls chdir/setenv/execvp/_exit on buffers built before the fork.
        let pid = unsafe { forkpty(&mut fd, null_mut(), null(), &ws) };
        if pid < 0 {
            return Err("forkpty failed".into());
        }
        if pid == 0 {
            unsafe {
                if let Some(d) = &cwd {
                    chdir(d.as_ptr());
                }
                setenv(term.as_ptr(), xt.as_ptr(), 1);
                setenv(ct.as_ptr(), tc.as_ptr(), 1);
                execvp(prog.as_ptr(), argv.as_ptr());
                _exit(127)
            }
        }
        // SAFETY: dup gives the reader thread its own fd, owned by the File.
        let mut f = unsafe { File::from_raw_fd(dup(fd)) };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut b = [0u8; 8192];
            while let Ok(n @ 1..) = f.read(&mut b) {
                if tx.send(b[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        Ok(Pty { fd, pid, rx, reaped: false })
    }

    pub fn write(&mut self, bytes: &[u8]) {
        let mut b = bytes;
        while !b.is_empty() {
            // SAFETY: valid buffer and length; fd is the pty master.
            let n = unsafe { write(self.fd, b.as_ptr() as *const c_void, b.len()) };
            if n <= 0 {
                break; // shortcut: a full pty buffer drops the rest, retry with poll if pastes get big
            }
            b = &b[n as usize..];
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let ws = Winsize { row: rows, col: cols, xpx: 0, ypx: 0 };
        // SAFETY: TIOCSWINSZ reads one `winsize`.
        unsafe { ioctl(self.fd, TIOCSWINSZ, &ws as *const Winsize) };
    }

    /// Everything the child printed since the last call; never blocks.
    pub fn try_read(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        while let Ok(v) = self.rx.try_recv() {
            out.extend(v);
        }
        out
    }

    pub fn alive(&mut self) -> bool {
        if !self.reaped {
            let mut st = 0;
            // SAFETY: plain waitpid on our own child.
            self.reaped = unsafe { waitpid(self.pid, &mut st, WNOHANG) } == self.pid;
        }
        !self.reaped
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        if !self.reaped {
            // SAFETY: our child; SIGKILL then reap so it never lingers.
            unsafe {
                kill(self.pid, SIGKILL);
                waitpid(self.pid, null_mut(), 0);
            }
        }
        unsafe { close(self.fd) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vt::Vt;

    /// Runs `sh -c script` to completion and returns what a 40x6 Vt shows.
    fn run(script: &str) -> Vt {
        let mut p = Pty::spawn("sh", &["-c", script], None, 40, 6).unwrap();
        let mut vt = Vt::new(40, 6);
        let t = std::time::Instant::now();
        while p.alive() && t.elapsed().as_secs() < 5 {
            vt.feed(&p.try_read());
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        vt.feed(&p.try_read());
        vt
    }

    #[test]
    fn printf_sgr_renders() {
        let vt = run("printf '\\033[31mred\\033[0m ok\\n'");
        assert!(vt.row_text(0).starts_with("red ok"), "{:?}", vt.row_text(0));
    }

    #[test]
    #[ignore = "needs ls colour and the claude binary"]
    fn real_programs() {
        let vt = run("CLICOLOR_FORCE=1 ls -G /");
        assert!(vt.row_text(0).trim().len() > 2);
        let vt = run("claude --version");
        assert!(vt.row_text(0).contains("Claude Code"), "{:?}", vt.row_text(0));
    }
}
