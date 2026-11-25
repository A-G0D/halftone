//! The workspace file: plain lines a person or an agent can edit; the app reloads on save.

use std::path::PathBuf;

pub const DEFAULT: &str = "\
# halftone desk. Edit it, or run `halftone setup` and Claude builds it with you. Saves reload live.
# tab <any name>, then rows: row <height|fill [weight]>: <card> [args] [weight], ...
# themes: ink, paper, mocha, latte, tokyo, rosepine, dawn, gruvbox, nord, kanagawa, everforest (press t to cycle)
# cards: start, media, apps, agents, watch [1x2|2x2|1x4|2x4], revenue, x, term <command>, player <mode>
# modules: any script in modules/ next to this file. Press a on the desk to browse and add them,
#   or run  halftone module list.  halftone module new <name> makes your own.
theme ink
agent claude

tab Home
row 4: media 3, apps 2
row 13: start 2, agents 3
row 10: weather, system, clocks
row fill: watch 4x1

tab Dev
row fill: github 3, git 2
row fill: ci 2, claude 2, ports 2

tab Money
row 10: indices, futures, rates
row fill: watch 2x2 3, crypto 2
row fill: revenue 3, forex 2

tab Read
row fill: hackernews 3, rss 2
row fill: onthisday 2, arxiv 2, sports 2

# media <label> <file, url or ytsearch1:query> [bg <video>] [from <m:ss>]
media Music ~/.cache/halftone/media/nightowl.m4a
media Starman ~/.cache/halftone/media/roadster.mp4
media Falcon ~/.cache/halftone/media/falcon9.mp4

watch TSLA NVDA AAPL PLTR

# Linking apps: run  halftone setup  (or halftone setup stocks|discord|gmail|stripe|x).
# It writes these lines for you. Discord and Mail are read from their apps with no line at all.
# connect <service> <token, $ENV_VAR or @/file> [more]: live data instead of samples.
# connect discord @app              (Discord desktop app open and signed in: its unread badge)
# connect gmail @app                (Apple Mail with your Gmail added: its unread badge)
# connect gmail you@example.com $GMAIL_APP_PASSWORD   (myaccount.google.com/apppasswords)
# connect stripe $STRIPE_KEY        (a restricted key that can read invoices)
# connect x @signin                 (run: halftone setup x)

# key <combo> send <text> | tab <name> | run <command> | theme <name> | voice
key ctrl+t send cargo test\\r
key alt+v voice

# mods: voice, lyrics, youtube, island, toasts (halftone mod list)
mod island
mod toasts
mod youtube
";

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub kind: String,
    pub args: Vec<String>,
    pub weight: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Height {
    Len(u16),
    Fill(u16),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub h: Height,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub name: String,
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Media {
    pub label: String,
    pub src: String,
    pub bg: Option<String>,
    /// Seconds into the source to start from.
    pub from: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Config {
    pub theme: String,
    pub agent: String,
    pub tabs: Vec<Tab>,
    pub media: Vec<Media>,
    pub watch: Vec<String>,
    pub keys: Vec<(String, String)>,
    pub mods: Vec<String>,
    /// (service, args): `connect x $X_USER_TOKEN 12345`.
    pub connects: Vec<(String, Vec<String>)>,
}

pub fn path() -> PathBuf {
    if let Ok(p) = std::env::var("HALFTONE_CONFIG") {
        return p.into();
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config/halftone/desk.conf")
}

/// Reads the file, writing the default one first if there is none.
pub fn load() -> Result<Config, String> {
    let p = path();
    if !p.exists() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(&p, DEFAULT).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    parse(&text)
}

pub fn expand(s: &str) -> String {
    match s.strip_prefix("~/") {
        Some(rest) => format!("{}/{rest}", std::env::var("HOME").unwrap_or_default()),
        None => s.to_string(),
    }
}

fn item(s: &str) -> Option<Item> {
    let mut words: Vec<String> = s.split_whitespace().map(String::from).collect();
    let weight = match words.last().and_then(|w| w.parse::<u16>().ok()) {
        Some(n) if words.len() > 1 => {
            words.pop();
            n.max(1)
        }
        _ => 1,
    };
    let kind = words.first()?.clone();
    Some(Item { kind, args: words[1..].to_vec(), weight })
}

/// `90`, `1:30` or `1:02:03` as seconds.
fn clock(t: &str) -> Option<f64> {
    t.split(':').try_fold(0.0, |acc, p| p.parse::<f64>().ok().map(|n| acc * 60.0 + n))
}

pub fn parse(text: &str) -> Result<Config, String> {
    let mut c = Config { theme: "ink".into(), agent: "claude".into(), ..Config::default() };
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let err = |m: &str| format!("line {}: {m}: {line}", n + 1);
        let (head, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        match head {
            "theme" => c.theme = rest.into(),
            "agent" => c.agent = rest.into(),
            "tab" => c.tabs.push(Tab { name: rest.into(), rows: vec![] }),
            "row" => {
                let (h, items) = rest.split_once(':').ok_or_else(|| err("row needs ':'"))?;
                let hw: Vec<&str> = h.split_whitespace().collect();
                let h = match hw.as_slice() {
                    ["fill"] => Height::Fill(1),
                    ["fill", w] => Height::Fill(w.parse().map_err(|_| err("bad fill weight"))?),
                    [n] => Height::Len(n.parse().map_err(|_| err("height is a number or fill"))?),
                    _ => return Err(err("height is a number or fill")),
                };
                let items: Vec<Item> = items.split(',').filter_map(item).collect();
                let tab = c.tabs.last_mut().ok_or_else(|| err("row before any tab"))?;
                tab.rows.push(Row { h, items });
            }
            "media" => {
                // The source may hold spaces (ytsearch1:a few words); ` bg ` splits off a video.
                let (label, src) = rest.split_once(char::is_whitespace).ok_or_else(|| err("media needs a label and a source"))?;
                let (src, from) = match src.rsplit_once(" from ") {
                    Some((s, t)) => (s, clock(t.trim()).ok_or_else(|| err("from is m:ss or seconds"))?),
                    None => (src, 0.0),
                };
                let (src, bg) = match src.split_once(" bg ") {
                    Some((s, b)) => (s.trim(), Some(b.trim().to_string())),
                    None => (src.trim(), None),
                };
                c.media.push(Media { label: label.into(), src: src.into(), bg, from });
            }
            "watch" => c.watch = rest.split_whitespace().map(|s| s.to_uppercase()).collect(),
            "key" => {
                let (combo, action) = rest.split_once(char::is_whitespace).ok_or_else(|| err("key needs an action"))?;
                c.keys.push((combo.into(), action.trim().into()));
            }
            "mod" => c.mods.push(rest.into()),
            "connect" => {
                let mut w = rest.split_whitespace().map(String::from);
                let service = w.next().ok_or_else(|| err("connect needs a service"))?;
                c.connects.push((service.to_lowercase(), w.collect()));
            }
            _ => return Err(err("unknown setting")),
        }
    }
    if c.tabs.is_empty() {
        return Err("no tabs".into());
    }
    Ok(c)
}

/// Writes the layout back after the editor changes it, keeping everything else as it was.
pub fn save(c: &Config) -> Result<(), String> {
    let p = path();
    let old = std::fs::read_to_string(&p).unwrap_or_default();
    let mut out = String::new();
    let mut wrote_tabs = false;
    let mut in_tabs = false;
    for line in old.lines() {
        let t = line.trim_start();
        let tabby = t.starts_with("tab ") || t.starts_with("row ");
        if tabby && !wrote_tabs {
            out.push_str(&tabs(c));
            wrote_tabs = true;
        }
        if tabby {
            in_tabs = true;
            continue;
        }
        if in_tabs && t.is_empty() {
            continue;
        }
        in_tabs = false;
        out.push_str(line);
        out.push('\n');
    }
    if !wrote_tabs {
        out.push_str(&tabs(c));
    }
    std::fs::write(&p, out).map_err(|e| format!("{}: {e}", p.display()))
}

fn tabs(c: &Config) -> String {
    let mut s = String::new();
    for t in &c.tabs {
        s += &format!("tab {}\n", t.name);
        for r in &t.rows {
            let h = match r.h {
                Height::Len(n) => n.to_string(),
                Height::Fill(1) => "fill".into(),
                Height::Fill(w) => format!("fill {w}"),
            };
            let items: Vec<String> = r.items.iter().map(|i| {
                let mut w = vec![i.kind.clone()];
                w.extend(i.args.iter().cloned());
                w.push(i.weight.to_string());
                w.join(" ")
            }).collect();
            s += &format!("row {h}: {}\n", items.join(", "));
        }
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_parses_and_round_trips_layout() {
        let c = parse(DEFAULT).unwrap();
        assert_eq!(c.tabs.len(), 4);
        assert_eq!(c.tabs[0].rows[0].h, Height::Len(4));
        assert_eq!(c.tabs[0].rows[0].items[0], Item { kind: "media".into(), args: vec![], weight: 3 });
        assert_eq!(c.tabs[0].rows[1].items[0].kind, "start");
        assert!(c.connects.is_empty());
        assert_eq!(parse("tab A\nconnect X $T 42").unwrap().connects, vec![("x".to_string(), vec!["$T".to_string(), "42".to_string()])]);
        assert_eq!(c.media[1].label, "Starman");
        assert_eq!(parse("tab A\nmedia X a b bg c.mp4 from 1:02:03").unwrap().media[0], Media { label: "X".into(), src: "a b".into(), bg: Some("c.mp4".into()), from: 3723.0 });
        let again = parse(&tabs(&c)).unwrap();
        assert_eq!(again.tabs, c.tabs);
    }

    #[test]
    fn errors_name_the_line() {
        assert!(parse("tab A\nrow x: media").unwrap_err().starts_with("line 2"));
    }
}
