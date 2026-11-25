//! `halftone setup [item]`: one guided pass over everything the desk can link, or a single item.
//! Each step says what it needs, opens the app or page that gives it, checks what you typed and
//! writes the desk file. Enter skips a step. Secrets go to their own files (mode 600), never into
//! the desk file.

use super::{cli, config, connect, quotes, xauth};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub const ITEMS: [(&str, &str); 7] = [
    ("stocks", "type the tickers to watch (Enter keeps the current ones)"),
    ("discord", "show Discord unreads: just needs the Discord app open"),
    ("gmail", "show unread mail: add Google in Mail, or paste an app password"),
    ("stripe", "show invoices and income: paste a read-only key"),
    ("x", "show your trends and top posts: sign in to X"),
    ("path", "make `halftone` work from any folder"),
    ("layout", "let Claude shape your tabs and cards with you"),
];

/// Whether a setup item is already linked, from the desk file and the feeds `connects` yields.
pub fn linked(item: &str, cfg: &config::Config, connects: &[(String, Vec<String>)]) -> bool {
    match item {
        "stocks" => !cfg.watch.is_empty(),
        "x" => connects.iter().any(|(s, _)| s == "x") || xauth::Tok::load().is_some(),
        "theme" => cfg.theme != "ink",
        "modules" => cfg.tabs.iter().flat_map(|t| &t.rows).flat_map(|r| &r.items).any(|i| super::modules::exists(&i.kind)),
        // Mail is linked on its own whenever it is installed; only a setup line means Google is in it.
        "gmail" => cfg.connects.iter().any(|(s, _)| s == "gmail"),
        _ => connects.iter().any(|(s, _)| s == item),
    }
}

pub fn run(args: &[String]) -> Result<(), String> {
    let cfg = config::load()?;
    match args.first().map(String::as_str) {
        None => {
            println!("halftone setup · {} steps, press Enter to skip any of them.", ITEMS.len());
            println!("Your desk file: {}\n", config::path().display());
            let conn = super::connects(&cfg, false);
            for (i, (name, what)) in ITEMS.iter().enumerate() {
                let done = linked(name, &cfg, &conn);
                println!("\x1b[1m{}/{}  {name}\x1b[0m{} · {what}", i + 1, ITEMS.len(), if done { " (done)" } else { "" });
                if done && !yes("  Already linked. Redo it? [y/N] ") {
                    println!();
                    continue;
                }
                if let Err(e) = step(name) {
                    println!("  skipped: {e}");
                }
                println!();
            }
            println!("Done. Run \x1b[1mhalftone\x1b[0m to open your desk; redo one step with  halftone setup <step>.");
            Ok(())
        }
        Some(item) if ITEMS.iter().any(|(n, _)| *n == item) => step(item),
        Some(other) => {
            println!("No setup step called {other:?}. Steps:");
            for (n, what) in ITEMS {
                println!("  halftone setup {n:<8} {what}");
            }
            Ok(())
        }
    }
}

fn step(item: &str) -> Result<(), String> {
    match item {
        "stocks" => stocks(),
        "discord" => discord(),
        "gmail" => gmail(),
        "stripe" => stripe(),
        "x" => x(),
        "path" => path(),
        _ => layout(),
    }
}

fn ask(prompt: &str) -> String {
    print!("  {prompt}");
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    let _ = std::io::stdin().lock().read_line(&mut s);
    s.trim().to_string()
}

/// A line read with the terminal's echo off.
fn secret(prompt: &str) -> String {
    let stty = |a: &str| _ = Command::new("stty").arg(a).stdin(Stdio::inherit()).status();
    stty("-echo");
    let s = ask(prompt);
    stty("echo");
    println!();
    s
}

fn yes(prompt: &str) -> bool {
    ask(prompt).to_lowercase().starts_with('y')
}

fn open(target: &str, app: bool) {
    let mut c = Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" });
    if app {
        c.arg("-a");
    }
    let _ = c.arg(target).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

/// Replaces the desk file's lines that start with `prefix` by `line` (or just drops them when
/// `line` is empty), keeping everything else as written.
fn set_line(prefix: &str, line: &str) -> Result<(), String> {
    let p = config::path();
    let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    let mut out: Vec<String> = text.lines().filter(|l| !l.starts_with(prefix)).map(String::from).collect();
    if !line.is_empty() {
        out.push(line.to_string());
    }
    let new = out.join("\n") + "\n";
    config::parse(&new)?;
    std::fs::write(&p, new).map_err(|e| e.to_string())
}

/// Writes a secret beside the desk file, readable only by you; `connect` lines point at it.
fn keep_secret(name: &str, value: &str) -> Result<String, String> {
    let p = config::path().with_file_name(name);
    std::fs::write(&p, value).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    Ok(format!("@{}", p.display()))
}

/// The watchlist grid that fits `n` tickers: columns × rows.
pub fn grid_for(n: usize) -> &'static str {
    match n { 0..=1 => "1x1", 2 => "1x2", 3..=4 => "2x2", 5..=6 => "2x3", _ => "2x4" }
}

/// Points every `watch` card at `grid`, keeping each card's weight.
pub fn regrid(text: &str, grid: &str) -> String {
    text.lines().map(|l| {
        let Some(rest) = l.strip_prefix("row ") else { return l.to_string() };
        let Some((h, items)) = rest.split_once(':') else { return l.to_string() };
        let items: Vec<String> = items.split(',').map(|it| {
            let words: Vec<&str> = it.split_whitespace().collect();
            if words.first() != Some(&"watch") {
                return it.trim().to_string();
            }
            let weight = words.last().filter(|w| words.len() > 1 && w.parse::<u16>().is_ok());
            match weight { Some(w) => format!("watch {grid} {w}"), None => format!("watch {grid}") }
        }).collect();
        format!("row {h}: {}", items.join(", "))
    }).collect::<Vec<_>>().join("\n") + "\n"
}

fn stocks() -> Result<(), String> {
    let now = config::load()?.watch.join(" ");
    let got = ask(&format!("Tickers, up to 8 [{}]: ", if now.is_empty() { "TSLA NVDA AAPL PLTR" } else { &now }));
    if got.is_empty() {
        return Ok(());
    }
    let mut ok = vec![];
    for s in got.split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()).take(8) {
        let s = s.to_uppercase();
        match quotes::fetch(&s, 0) {
            Some(q) => {
                println!("  {s:<6} {:>9.2}  {:+.2}%  {}", q.price, (q.price / q.base - 1.0) * 100.0, q.name);
                ok.push(s);
            }
            None => println!("  {s:<6} not found, left out"),
        }
    }
    if ok.is_empty() {
        return Err("no tickers found".into());
    }
    set_line("watch ", &format!("watch {}", ok.join(" ")))?;
    let p = config::path();
    let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    std::fs::write(&p, regrid(&text, grid_for(ok.len()))).map_err(|e| e.to_string())?;
    println!("  Watching {} as a {} grid.", ok.join(" "), grid_for(ok.len()));
    Ok(())
}

fn discord() -> Result<(), String> {
    if !std::path::Path::new("/Applications/Discord.app").exists() {
        println!("  The desk reads the Discord app's Dock badge, so it needs the desktop app.");
        if yes("Open discord.com/download? [y/N] ") {
            open("https://discord.com/download", false);
        }
        println!("  Install it, sign in, then run  halftone setup discord  again.");
        return Ok(());
    }
    set_line("connect discord", "connect discord @app")?;
    open("Discord", true);
    println!("  Linked: the Discord row shows the number on Discord's Dock icon while the app is open.");
    println!("  No dot on the icon for unreads? Turn on Discord › Settings › Notifications › Enable Unread Message Badge");
    println!("  and System Settings › Notifications › Discord › Badge application icon.");
    Ok(())
}

fn gmail() -> Result<(), String> {
    println!("  1  Apple Mail: sign in to Google once, the desk reads Mail's unread badge (easiest)");
    println!("  2  App password: the desk checks Gmail directly; needs 2-step verification on");
    match ask("1 or 2 [Enter skips]: ").as_str() {
        "1" => {
            open("x-apple.systempreferences:com.apple.Internet-Accounts-Settings.extension", false);
            println!("  In Internet Accounts: Google › sign in › keep Mail ticked.");
            ask("Press Enter when Mail shows your inbox… ");
            open("Mail", true);
            set_line("connect gmail", "connect gmail @app")?;
            println!("  Linked: the Gmail row shows Mail's unread count while Mail is open.");
            Ok(())
        }
        "2" => {
            open("https://myaccount.google.com/apppasswords", false);
            println!("  Create an app password named halftone (Google shows 16 letters).");
            let email = ask("Gmail address: ");
            if !email.contains('@') {
                return Err("no address".into());
            }
            let pass = secret("App password (hidden): ").replace(' ', "");
            if pass.is_empty() {
                return Err("no password".into());
            }
            let file = keep_secret("gmail.pass", &pass)?;
            let args = vec![email.clone(), file.clone()];
            match connect::probe("gmail", &args) {
                Ok(s) => println!("  Signed in: {s}."),
                Err(e) => return Err(e),
            }
            set_line("connect gmail", &format!("connect gmail {email} {file}"))
        }
        _ => Ok(()),
    }
}

fn stripe() -> Result<(), String> {
    println!("  Needs a restricted key that can read invoices (nothing else).");
    if !yes("Open the Stripe dashboard to make one? [y/N] ") {
        return Ok(());
    }
    open("https://dashboard.stripe.com/apikeys", false);
    println!("  Create restricted key › Invoices: Read › Create, then copy it.");
    let key = secret("Key (hidden): ");
    if key.is_empty() {
        return Ok(());
    }
    let file = keep_secret("stripe.key", &key)?;
    let s = connect::probe("stripe", std::slice::from_ref(&file))?;
    println!("  Connected: {s}.");
    set_line("connect stripe", &format!("connect stripe {file}"))
}

fn x() -> Result<(), String> {
    if xauth::Tok::load().is_some() {
        println!("  Already signed in to X.");
        return Ok(());
    }
    println!("  Needs a free X developer app; X bills its API per post read (about $0.005).");
    if !yes("Set it up now? [y/N] ") {
        return Ok(());
    }
    xauth::connect(&[])
}

/// Copies this binary into a folder on your PATH, so `halftone` works from anywhere.
fn path() -> Result<(), String> {
    // npm's launcher sets this; npm owns the command and updates it, so a copy would go stale.
    if std::env::var_os("HALFTONE_NPM").is_some() {
        println!("  Installed through npm, which puts `halftone` on your PATH (npm i -g halftone-cli).");
        return Ok(());
    }
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let on_path: Vec<PathBuf> = std::env::var("PATH").unwrap_or_default().split(':').map(PathBuf::from).collect();
    if me.parent().is_some_and(|d| on_path.iter().any(|p| p == d)) {
        println!("  Already on your PATH: {}.", me.display());
        return Ok(());
    }
    let dir = [home.join(".local/bin"), home.join(".cargo/bin"), "/opt/homebrew/bin".into(), "/usr/local/bin".into()]
        .into_iter()
        .find(|d| on_path.contains(d) && std::fs::metadata(d).is_ok_and(|m| !m.permissions().readonly()));
    let Some(dir) = dir else {
        let d = home.join(".local/bin");
        println!("  No writable folder on your PATH. Add this to your shell profile, then rerun:");
        println!("    export PATH=\"{}:$PATH\"", d.display());
        return Ok(());
    };
    let dest = dir.join("halftone");
    if dest == me {
        println!("  Already installed at {}.", dest.display());
        return Ok(());
    }
    let verb = if dest.exists() { "Replace the halftone in" } else { "Install halftone into" };
    if !yes(&format!("{verb} {}? [y/N] ", dir.display())) {
        return Ok(());
    }
    // A copy, not a link: it keeps working when the build folder moves.
    let tmp = dir.join(".halftone.new");
    std::fs::copy(&me, &tmp).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    println!("  Installed: {}", dest.display());
    Ok(())
}

fn layout() -> Result<(), String> {
    if !cli::has("claude") {
        println!("  Needs Claude Code (claude.com/claude-code). Or edit the desk file by hand: halftone config");
        return Ok(());
    }
    if !yes("Shape your tabs and cards with Claude now? [y/N] ") {
        return Ok(());
    }
    let agent = config::load().map(|c| c.agent).unwrap_or_else(|_| "claude".into());
    crate::term::run(Box::new(super::wrap::Wrap::new(&cli::setup_cmd(&agent))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_cards_take_the_grid() {
        let t = "row 4: media 3, apps 2\nrow fill: agents 3, watch 2\nrow fill: watch 1x2, x\n";
        assert_eq!(regrid(t, "2x2"), "row 4: media 3, apps 2\nrow fill: agents 3, watch 2x2 2\nrow fill: watch 2x2, x\n");
        assert_eq!((grid_for(2), grid_for(4), grid_for(8)), ("1x2", "2x2", "2x4"));
    }
}
