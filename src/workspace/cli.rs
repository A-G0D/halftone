//! Commands for people and agents: `notify`, `config`, `mod`. Everything they change lands in plain
//! files the running workspace watches, so an agent can drive the desk with shell calls.

use super::config;
use std::io::Write;
use std::process::Command;

/// (name, what it does, programs it needs, needs the whisper model)
const MODS: [(&str, &str, &[&str], bool); 5] = [
    ("voice", "talk into Claude Code or any terminal (ctrl+space)", &["ffmpeg", "whisper-cli"], true),
    ("lyrics", "live lyrics and captions from whisper when there is no .lrc", &["whisper-cli"], true),
    ("youtube", "play links and ytsearch1: queries through yt-dlp", &["yt-dlp"], false),
    ("island", "agent activity in a pill at the top", &[], false),
    ("toasts", "notifications from apps and `halftone notify`", &[], false),
];

const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";

pub fn has(bin: &str) -> bool {
    Command::new("sh").args(["-c", &format!("command -v {bin}")]).output().is_ok_and(|o| o.status.success())
}

fn hint(bin: &str) -> &'static str {
    match bin {
        "whisper-cli" => "brew install whisper-cpp",
        "yt-dlp" => "brew install yt-dlp",
        _ => "brew install ffmpeg",
    }
}

pub fn notify(args: &[String]) -> Result<(), String> {
    let [app, title, rest @ ..] = args else {
        return Err("usage: halftone notify <app> <title> [body]".into());
    };
    let clean = |s: &str| s.replace(['\t', '\n'], " ");
    let line = format!("{}\t{}\t{}\n", clean(app), clean(title), clean(&rest.join(" ")));
    let p = super::inbox();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&p).map_err(|e| e.to_string())?;
    f.write_all(line.as_bytes()).map_err(|e| e.to_string())
}

/// `halftone module list | new NAME | run NAME`: script cards, built so an agent can write and debug one.
pub fn module_cmd(args: &[String]) -> Result<(), String> {
    use super::modules as m;
    m::install();
    match (args.first().map(String::as_str), args.get(1)) {
        (Some("list") | None, _) => {
            let mut names: Vec<String> = std::fs::read_dir(m::dir()).map_err(|e| e.to_string())?.flatten().filter(|e| e.path().is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            for n in names {
                println!("{n:<12} {}  [{}]", m::path(&n).display(), m::status(&n));
            }
        }
        (Some("new"), Some(name)) => {
            if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Err("module names are letters, digits, - and _".into());
            }
            if m::exists(name) {
                return Err(format!("{} already exists", m::path(name).display()));
            }
            let p = m::path(name);
            std::fs::write(&p, m::SCAFFOLD.replace("NAME", name)).map_err(|e| e.to_string())?;
            m::make_exec(&p);
            println!("wrote {}\nadd to your desk (halftone config path):  row 8: {name}\ntest it:  halftone module run {name}", p.display());
        }
        (Some("run"), Some(name)) => {
            let o = m::exec(name)?;
            println!("title: {:?}  every: {}s  open: {:?}", o.title, o.every, o.open);
            for l in &o.lines {
                println!("{l:?}");
            }
        }
        _ => return Err("usage: halftone module list | new NAME | run NAME".into()),
    }
    Ok(())
}

pub fn config_cmd(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("path") | None => println!("{}", config::path().display()),
        Some("check") => {
            let c = config::load()?;
            println!("ok: {} tabs, {} media, {} keys, mods: {}", c.tabs.len(), c.media.len(), c.keys.len(), c.mods.join(" "));
        }
        Some("print") => {
            config::load()?;
            print!("{}", std::fs::read_to_string(config::path()).map_err(|e| e.to_string())?);
        }
        Some("reset") => {
            std::fs::write(config::path(), config::DEFAULT).map_err(|e| e.to_string())?;
            println!("wrote the default layout to {}", config::path().display());
        }
        Some(o) => return Err(format!("unknown: config {o}. try path, check, print, reset")),
    }
    Ok(())
}

pub fn mod_cmd(args: &[String]) -> Result<(), String> {
    let c = config::load()?;
    match (args.first().map(String::as_str), args.get(1)) {
        (Some("list") | None, _) => {
            for (name, what, bins, model) in MODS {
                let on = c.mods.iter().any(|m| m == name);
                let missing: Vec<&str> = bins.iter().copied().filter(|b| !has(b)).collect();
                let model_missing = model && !crate::media::whisper_model().exists();
                let state = if !missing.is_empty() || model_missing { "needs setup" } else if on { "on" } else { "off" };
                println!("{:<9} {:<12} {what}", name, state);
            }
            println!("\nhalftone mod add <name>   downloads what it needs and turns it on");
        }
        (Some("add"), Some(name)) => {
            let Some(&(_, _, bins, model)) = MODS.iter().find(|m| m.0 == name) else {
                return Err(format!("no mod called {name}"));
            };
            for b in bins {
                if !has(b) {
                    return Err(format!("{name} needs {b}: {}", hint(b)));
                }
            }
            let mp = crate::media::whisper_model();
            if model && !mp.exists() {
                println!("downloading the whisper model (142 MB) to {}", mp.display());
                if let Some(d) = mp.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                let ok = Command::new("curl").args(["-fL", "--progress-bar", "-o"]).arg(&mp).arg(MODEL_URL).status().is_ok_and(|s| s.success());
                if !ok {
                    let _ = std::fs::remove_file(&mp);
                    return Err("download failed".into());
                }
            }
            if !c.mods.iter().any(|m| m == name) {
                let mut f = std::fs::OpenOptions::new().append(true).open(config::path()).map_err(|e| e.to_string())?;
                writeln!(f, "mod {name}").map_err(|e| e.to_string())?;
            }
            println!("{name} is on");
        }
        (Some("rm"), Some(name)) => {
            let text = std::fs::read_to_string(config::path()).map_err(|e| e.to_string())?;
            let kept: Vec<&str> = text.lines().filter(|l| l.trim() != format!("mod {name}")).collect();
            std::fs::write(config::path(), kept.join("\n") + "\n").map_err(|e| e.to_string())?;
            println!("{name} is off");
        }
        _ => return Err("usage: halftone mod [list | add <name> | rm <name>]".into()),
    }
    Ok(())
}

/// Claude's opening brief for `halftone setup`. No single quotes: it is passed through a shell.
pub fn setup_cmd(agent: &str) -> String {
    let brief = format!(
        "You are setting up halftone, a desk that lives in the terminal. Its whole layout is one plain file at {} \
(run: halftone config print. The comments in it explain the format). Interview me briefly, one short question at a time: \
what to call my tabs and what each is for; which cards each tab needs (media, apps for Gmail and Discord, agents, \
watch with tickers, revenue, x for my X feed, term followed by any command for a terminal app, player); which tickers to watch; \
what music or video to play; and whether to connect Gmail, Discord or X. For each connection, say where to get the token \
and put an environment variable name such as $X_USER_TOKEN in the file, never the token itself. Then write the file, \
run: halftone config check, fix anything it reports, and tell me to type /exit to open my desk.",
        config::path().display()
    );
    format!("{agent} \"{}\"", brief.replace('"', "").replace('$', "\\$"))
}

/// `halftone lyrics <file>`: whisper writes `<file>.lrc` beside it; players pick it up.
pub fn lyrics(args: &[String]) -> Result<(), String> {
    let src = args.first().ok_or("usage: halftone lyrics <audio or video file>")?;
    let model = crate::media::whisper_model();
    if !model.exists() {
        return Err("needs the whisper model: halftone mod add lyrics".into());
    }
    let p = std::path::Path::new(src);
    let wav = std::env::temp_dir().join("halftone-lyrics.wav");
    let ok = Command::new("ffmpeg").args(["-v", "error", "-y", "-i", src, "-ar", "16000", "-ac", "1"]).arg(&wav).status().is_ok_and(|s| s.success());
    if !ok {
        return Err(format!("ffmpeg could not read {src}"));
    }
    let stem = p.with_extension("");
    let ok = Command::new("whisper-cli").arg("-m").arg(&model).arg("-f").arg(&wav).args(["-olrc", "-np", "-of"]).arg(&stem)
        .stdout(std::process::Stdio::null()).status().is_ok_and(|s| s.success());
    let _ = std::fs::remove_file(&wav);
    if !ok {
        return Err("whisper-cli failed".into());
    }
    println!("wrote {}", p.with_extension("lrc").display());
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn setup_brief_survives_the_shell() {
        let c = super::setup_cmd("claude");
        assert!(c.starts_with("claude \"") && c.ends_with('"'));
        assert!(c.contains("\\$X_USER_TOKEN") && !c[8..c.len() - 1].contains('"'));
    }
}
