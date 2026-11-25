//! Drives a program through a real PTY and prints its screen after each step, so key handling
//! can be checked without anyone at a keyboard: `cargo run --example drive -- <bin> < steps`.
//! Steps, one per line: `key <text>` (\e \r \t \x0b escapes), `wait <secs>`, `show <label>`.

#[cfg(unix)]
fn main() {
    use halftone::{pty::Pty, vt::Vt};
    use std::io::BufRead;
    use std::time::{Duration, Instant};

    let bin = std::env::args().nth(1).expect("usage: drive <program> < steps");
    let (cols, rows) = (160, 48);
    let mut pty = Pty::spawn(&bin, &[], None, cols, rows).expect("spawn");
    let mut vt = Vt::new(cols, rows);
    let pump = |pty: &mut Pty, vt: &mut Vt, secs: f64| {
        let end = Instant::now() + Duration::from_secs_f64(secs);
        while Instant::now() < end {
            vt.feed(&pty.try_read());
            let r = vt.take_replies();
            if !r.is_empty() {
                pty.write(&r);
            }
            std::thread::sleep(Duration::from_millis(15));
        }
    };
    pump(&mut pty, &mut vt, 2.0);
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let (op, arg) = line.split_once(' ').unwrap_or((&line, ""));
        match op {
            "key" => {
                let bytes = arg.replace("\\e", "\x1b").replace("\\r", "\r").replace("\\t", "\t").replace("\\x0b", "\x0b").replace("\\x00", "\0");
                pty.write(bytes.as_bytes());
                pump(&mut pty, &mut vt, 0.6);
            }
            "wait" => pump(&mut pty, &mut vt, arg.parse().unwrap_or(1.0)),
            "show" => {
                println!("=== {arg}");
                for y in 0..rows {
                    let t = vt.row_text(y);
                    if !t.trim().is_empty() {
                        println!("{}", t.trim_end());
                    }
                }
            }
            _ => {}
        }
    }
    pty.write(b"q");
    pump(&mut pty, &mut vt, 0.5);
}

#[cfg(not(unix))]
fn main() {}
