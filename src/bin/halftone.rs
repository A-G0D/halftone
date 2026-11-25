//! `halftone` CLI: live dev server, static web build, image to ANSI, project scaffold. Std only.

use halftone::image::{self, Dither, Image, Mode, Opts};
use halftone::{term, Buffer, Rect};
#[path = "../pane.rs"]
mod pane;
#[path = "../statusline.rs"]
mod statusline;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime};
use std::{env, fs, thread};

const INDEX: &str = include_str!("../../web/index.html");
const RUNTIME: &str = include_str!("../../web/runtime.js");
const USAGE: &str = "halftone — your desk, in a terminal

  halftone                                                     open the workspace (tabs, widgets, terminals)
  halftone setup [STEP]                                        link everything in one pass, or one step:
                                                               stocks discord gmail stripe x path layout
  halftone secret set|get|rm NAME                              tokens for modules (Keychain on macOS); set reads stdin
  halftone wrap  CMD [ARGS]                                  run a CLI full screen with voice, shortcuts and toasts
  halftone notify APP TITLE [BODY]                             show a toast in the running workspace (agents: use this)
  halftone config [path|check|print|reset]                     the layout file the workspace reloads on save
  halftone mod   [list | add NAME | rm NAME]                   optional features, downloaded on demand
  halftone module [list | new NAME | run NAME]                 script cards: any executable in ~/.config/halftone/modules/ (see AGENTS.md)
  halftone lyrics FILE                                        timed lyrics beside a track, from whisper

  halftone dev   [--example NAME | --bin NAME] [--port 7878]   live browser preview, rebuilds on save
  halftone build [--example NAME | --bin NAME] [--out dist]    static site: index.html, runtime.js, app.wasm
  halftone img   FILE [--width N] [--mode half|quad|braille|blocks|ascii] [--dither none|bayer|floyd]
  halftone new   NAME                                          scaffold an app
  halftone statusline [--demo]                                 Claude Code statusLine: JSON on stdin -> one pill row
  halftone island                                              always-on agent pane, e.g. tmux split-window -v -l 4 'halftone island'

Run the terminal version with plain `cargo run`.";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("dev") => dev(&args[1..]),
        Some("build") => build_cmd(&args[1..]),
        Some("img") => img(&args[1..]),
        Some("new") => new(&args[1..]),
        Some("statusline") => Ok(statusline::run(&args[1..])),
        Some("island") => term::run(Box::new(pane::Pane::new())),
        None | Some("desk") => term::run(Box::new(halftone::workspace::Workspace::new())),
        Some("setup") => halftone::workspace::setup::run(&args[1..]),
        Some("wrap") if args.len() > 1 => term::run(Box::new(halftone::workspace::wrap::Wrap::new(&args[1..].join(" ")))),
        Some("connect") if args.get(1).map(String::as_str) == Some("x") => halftone::workspace::xauth::connect(&args[2..]),
        Some("connect") if args.len() > 2 => halftone::workspace::secret::connect(&args[1..]),
        Some("secret") => halftone::workspace::secret::cmd(&args[1..]),
        Some("notify") => halftone::workspace::cli::notify(&args[1..]),
        Some("config") => halftone::workspace::cli::config_cmd(&args[1..]),
        Some("mod") => halftone::workspace::cli::mod_cmd(&args[1..]),
        Some("module") => halftone::workspace::cli::module_cmd(&args[1..]),
        Some("lyrics") => halftone::workspace::cli::lyrics(&args[1..]),
        _ => {
            println!("{USAGE}");
            Ok(())
        }
    };
    if let Err(e) = r {
        eprintln!("halftone: {e}");
        std::process::exit(1);
    }
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

/// Cargo target selector passed through from `--example`/`--bin`.
fn target(args: &[String]) -> Vec<String> {
    ["--example", "--bin"]
        .iter()
        .filter_map(|f| flag(args, f).map(|v| vec![f.to_string(), v.to_string()]))
        .flatten()
        .collect()
}

/// Builds the app for wasm32 and returns the .wasm bytes, or the compiler's rendered errors.
fn build(sel: &[String]) -> Result<Vec<u8>, String> {
    let out = Command::new(env::var("CARGO").unwrap_or("cargo".into()))
        .args(["build", "--release", "--target", "wasm32-unknown-unknown", "--message-format=json-render-diagnostics"])
        .args(sel)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cargo: {e}"))?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        let keep: Vec<&str> = stderr.lines().filter(|l| !l.trim_start().starts_with("Compiling") && !l.trim_start().starts_with("Blocking")).collect();
        return Err(keep.join("\n"));
    }
    // shortcut: pulls the last ".wasm" path out of cargo's JSON by string search; breaks on paths with escaped quotes.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let path = stdout
        .lines()
        .filter(|l| l.contains("\"compiler-artifact\""))
        .filter_map(|l| {
            let end = l.rfind(".wasm\"")? + 5;
            let start = l[..end].rfind('"')? + 1;
            Some(l[start..end].replace("\\\\", "\\"))
        })
        .last()
        .ok_or("build produced no .wasm; pass --example NAME or --bin NAME")?;
    fs::read(&path).map_err(|e| format!("{path}: {e}"))
}

fn build_cmd(args: &[String]) -> Result<(), String> {
    let dir = PathBuf::from(flag(args, "--out").unwrap_or("dist"));
    let wasm = build(&target(args))?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let w = |name: &str, b: &[u8]| fs::write(dir.join(name), b).map_err(|e| format!("{name}: {e}"));
    w("index.html", INDEX.replace("<!--LIVE-->", "").as_bytes())?;
    w("runtime.js", RUNTIME.as_bytes())?;
    w("app.wasm", &wasm)?;
    println!("wrote {} (app.wasm {} KB)", dir.display(), wasm.len() / 1024);
    Ok(())
}

#[derive(Default)]
struct Live {
    version: u64,
    wasm: Arc<Vec<u8>>,
    err: String,
}

type Shared = Arc<(Mutex<Live>, Condvar)>;

fn dev(args: &[String]) -> Result<(), String> {
    let port: u16 = flag(args, "--port").unwrap_or("7878").parse().map_err(|_| "bad --port")?;
    let sel = target(args);
    let live: Shared = Arc::new((Mutex::new(Live::default()), Condvar::new()));
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("port {port}: {e}"))?;

    let l = live.clone();
    thread::spawn(move || watch(sel, l));
    println!("halftone dev → http://127.0.0.1:{port}  (watching src/ examples/ assets/)");
    for stream in listener.incoming().flatten() {
        let l = live.clone();
        thread::spawn(move || {
            let _ = serve(stream, l);
        });
    }
    Ok(())
}

/// Newest mtime and file count under the watched paths; either changing means "rebuild".
fn stamp(p: &Path, acc: &mut (SystemTime, usize)) {
    let Ok(md) = fs::metadata(p) else { return };
    if md.is_dir() {
        for e in fs::read_dir(p).into_iter().flatten().flatten() {
            stamp(&e.path(), acc);
        }
    } else {
        acc.0 = acc.0.max(md.modified().unwrap_or(SystemTime::UNIX_EPOCH));
        acc.1 += 1;
    }
}

fn watch(sel: Vec<String>, live: Shared) {
    let mut last = None;
    loop {
        let mut s = (SystemTime::UNIX_EPOCH, 0);
        for p in ["src", "examples", "assets", "web", "Cargo.toml"] {
            stamp(Path::new(p), &mut s);
        }
        if last != Some(s) {
            last = Some(s);
            let t = std::time::Instant::now();
            let r = build(&sel);
            let (lock, cv) = &*live;
            let mut st = lock.lock().unwrap();
            match r {
                Ok(w) => {
                    println!("  built in {:.1}s ({} KB)", t.elapsed().as_secs_f32(), w.len() / 1024);
                    st.wasm = Arc::new(w);
                    st.err.clear();
                }
                Err(e) => {
                    println!("  build failed:\n{e}");
                    st.err = e;
                }
            }
            st.version += 1;
            cv.notify_all();
        }
        thread::sleep(Duration::from_millis(400));
    }
}

fn respond(s: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) -> std::io::Result<()> {
    write!(s, "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len())?;
    s.write_all(body)
}

fn serve(mut s: TcpStream, live: Shared) -> std::io::Result<()> {
    let mut req = Vec::new();
    let mut chunk = [0u8; 2048];
    while !req.windows(4).any(|w| w == b"\r\n\r\n") && req.len() < 16384 {
        let n = s.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        req.extend_from_slice(&chunk[..n]);
    }
    let line = String::from_utf8_lossy(&req);
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let (route, query) = path.split_once('?').unwrap_or((&path, ""));
    let (lock, cv) = &*live;
    match route {
        "/" | "/index.html" => {
            let v = lock.lock().unwrap().version;
            let script = format!(
                "<script>{{const e=document.getElementById('err'),s=new EventSource('/__live?v={v}');\
                 s.addEventListener('reload',()=>location.reload());\
                 s.addEventListener('fail',m=>e.textContent=m.data);}}</script>"
            );
            respond(&mut s, "200 OK", "text/html; charset=utf-8", INDEX.replace("<!--LIVE-->", &script).as_bytes())
        }
        "/runtime.js" => respond(&mut s, "200 OK", "text/javascript", RUNTIME.as_bytes()),
        "/app.wasm" => {
            let w = lock.lock().unwrap().wasm.clone();
            if w.is_empty() {
                respond(&mut s, "503 Service Unavailable", "text/plain", b"still building")
            } else {
                respond(&mut s, "200 OK", "application/wasm", &w)
            }
        }
        "/__live" => {
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\r\n")?;
            let mut seen: u64 = query.strip_prefix("v=").and_then(|v| v.parse().ok()).unwrap_or(0);
            let mut st = lock.lock().unwrap();
            if !st.err.is_empty() {
                let msg = sse("fail", &st.err);
                drop(st);
                s.write_all(msg.as_bytes())?;
                st = lock.lock().unwrap();
            }
            loop {
                let msg = if st.version != seen {
                    seen = st.version;
                    if st.err.is_empty() { sse("reload", "") } else { sse("fail", &st.err) }
                } else {
                    let (g, to) = cv.wait_timeout(st, Duration::from_secs(15)).unwrap();
                    st = g;
                    if !to.timed_out() {
                        continue;
                    }
                    ": ping\n\n".into()
                };
                drop(st);
                s.write_all(msg.as_bytes())?;
                st = lock.lock().unwrap();
            }
        }
        _ => respond(&mut s, "404 Not Found", "text/plain", b"not found"),
    }
}

/// One SSE event; multi-line data becomes several `data:` lines, which the browser rejoins with `\n`.
fn sse(event: &str, data: &str) -> String {
    let mut m = format!("event: {event}\n");
    for l in data.lines().chain(data.is_empty().then_some("")) {
        m += &format!("data: {l}\n");
    }
    m + "\n"
}

fn img(args: &[String]) -> Result<(), String> {
    let file = args.first().filter(|a| !a.starts_with("--")).ok_or("usage: halftone img FILE")?;
    let pic = Image::decode(&fs::read(file).map_err(|e| format!("{file}: {e}"))?)?;
    let mode = match flag(args, "--mode").unwrap_or("half") {
        "half" => Mode::Half,
        "quad" => Mode::Quad,
        "braille" => Mode::Braille,
        "blocks" => Mode::Blocks,
        "ascii" => Mode::Ascii,
        m => return Err(format!("unknown mode {m}")),
    };
    let dither = match flag(args, "--dither").unwrap_or("bayer") {
        "none" => Dither::None,
        "bayer" => Dither::Bayer,
        "floyd" => Dither::Floyd,
        d => return Err(format!("unknown dither {d}")),
    };
    let w = match flag(args, "--width") {
        Some(v) => v.parse().map_err(|_| "bad --width")?,
        None => term::size().0.min(100),
    };
    let mut big = Buffer::new(w, 1000);
    let used = image::draw(&mut big, Rect::new(0, 0, w, 1000), &pic, Opts { mode, dither, contrast: 1.0 });
    let mut out = Buffer::new(used.w, used.h);
    for y in 0..used.h {
        for x in 0..used.w {
            *out.cell(x, y).unwrap() = big.get(used.x + x, used.y + y);
        }
    }
    let tc = env::var("COLORTERM").is_ok_and(|v| v == "truecolor" || v == "24bit");
    print!("{}", term::to_ansi(&out, tc));
    Ok(())
}

fn new(args: &[String]) -> Result<(), String> {
    let name = args.first().ok_or("usage: halftone new NAME")?;
    let dir = Path::new(name);
    if dir.exists() {
        return Err(format!("{name} already exists"));
    }
    fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
    let lib = env!("CARGO_MANIFEST_DIR").replace('\\', "/");
    let cargo = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nhalftone-ui = {{ path = \"{lib}\" }}\n\n[profile.release]\nopt-level = \"s\"\nlto = true\npanic = \"abort\"\n"
    );
    let main = include_str!("../../examples/hello.rs").replace("`cargo run --example hello`", "`cargo run`").replace("`halftone dev --example hello`", "`halftone dev`");
    fs::write(dir.join("Cargo.toml"), cargo).map_err(|e| e.to_string())?;
    fs::write(dir.join("src/main.rs"), main).map_err(|e| e.to_string())?;
    println!("created {name}/\n  cd {name} && cargo run       # terminal\n  cd {name} && halftone dev     # browser, live reload");
    Ok(())
}
