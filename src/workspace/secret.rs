//! Tokens for modules: `halftone secret set|get|rm NAME`, and `halftone connect NAME URL PROMPT...`
//! which opens the page, takes the paste and saves it. macOS keeps them in the Keychain (service
//! "halftone"); elsewhere they are files in `<config dir>/secrets/` (dir 700, file 600). The value
//! never goes through argv: it is read from stdin and, on a Mac, fed to `security -i` on its stdin.

use super::{config, modules};
use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn file(name: &str) -> PathBuf {
    config::path().parent().map_or_else(|| PathBuf::from("secrets"), |d| d.join("secrets")).join(name)
}

#[cfg(unix)]
fn chmod(p: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode));
}
#[cfg(not(unix))]
fn chmod(_: &std::path::Path, _: u32) {}

/// Double-quoted for `security -i`, which reads its commands shell-style.
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn set(name: &str, value: &str) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        let mut c = Command::new("security").arg("-i").stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| format!("security: {e}"))?;
        let line = format!("add-generic-password -U -s halftone -a {} -w {}\n", quote(name), quote(value));
        c.stdin.take().ok_or("security: no stdin")?.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
        let _ = c.wait();
        // `security -i` exits 0 even when a command fails, so read it back.
        return if get(name).as_deref() == Some(value) { Ok(()) } else { Err("the Keychain refused the secret".into()) };
    }
    let p = file(name);
    let d = p.parent().ok_or("no secrets dir")?;
    std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    chmod(d, 0o700);
    std::fs::write(&p, value).map_err(|e| format!("{}: {e}", p.display()))?;
    chmod(&p, 0o600);
    Ok(())
}

pub fn get(name: &str) -> Option<String> {
    let v = if cfg!(target_os = "macos") {
        let o = Command::new("security").args(["find-generic-password", "-s", "halftone", "-a", name, "-w"]).stdin(Stdio::null()).stderr(Stdio::null()).output().ok().filter(|o| o.status.success())?;
        String::from_utf8_lossy(&o.stdout).trim_end_matches('\n').to_string()
    } else {
        std::fs::read_to_string(file(name)).ok()?.trim_end_matches('\n').to_string()
    };
    Some(v)
}

pub fn rm(name: &str) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        let ok = Command::new("security").args(["delete-generic-password", "-s", "halftone", "-a", name]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
        return if ok { Ok(()) } else { Err(format!("no secret called {name}")) };
    }
    std::fs::remove_file(file(name)).map_err(|_| format!("no secret called {name}"))
}

/// One line from stdin, with the terminal's echo off when someone is typing.
fn read_value(prompt: &str) -> String {
    let tty = std::io::stdin().is_terminal();
    let stty = |a: &str| _ = Command::new("stty").arg(a).status();
    if tty {
        eprint!("{prompt}");
        let _ = std::io::stderr().flush();
        stty("-echo");
    }
    let mut s = String::new();
    let _ = std::io::stdin().lock().read_line(&mut s);
    if tty {
        stty("echo");
        eprintln!();
    }
    s.trim().to_string()
}

fn check(name: &str) -> Result<(), String> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) {
        return Err("secret names are letters, digits, - _ and .".into());
    }
    Ok(())
}

pub fn cmd(args: &[String]) -> Result<(), String> {
    let (verb, name) = (args.first().map(String::as_str), args.get(1).map(String::as_str));
    let usage = "usage: halftone secret set|get|rm NAME   (set reads the value from stdin)";
    let name = name.ok_or(usage)?;
    check(name)?;
    match verb {
        Some("set") => {
            let v = read_value(&format!("{name}: "));
            if v.is_empty() {
                return Err("no value given, nothing saved".into());
            }
            set(name, &v)
        }
        Some("get") => match get(name) {
            Some(v) => Ok(println!("{v}")),
            None => std::process::exit(1),
        },
        Some("rm") => rm(name),
        _ => Err(usage.into()),
    }
}

/// `halftone connect NAME URL PROMPT...`: opens the page, takes the paste, saves it.
pub fn connect(args: &[String]) -> Result<(), String> {
    let [name, url, prompt @ ..] = args else { return Err("usage: halftone connect NAME URL [what to paste]".into()) };
    check(name)?;
    let prompt = if prompt.is_empty() { "your key".to_string() } else { prompt.join(" ") };
    println!("\n  Opening {url} in your browser: copy {prompt} from there and paste it here (it stays hidden).\n");
    modules::open(url);
    let v = read_value("  paste: ");
    if v.is_empty() {
        return Err("nothing pasted, nothing saved".into());
    }
    set(name, &v)?;
    println!("  Saved — back to your desk.");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn quotes_for_security() {
        assert_eq!(super::quote(r#"a"b\c"#), r#""a\"b\\c""#);
    }
}
