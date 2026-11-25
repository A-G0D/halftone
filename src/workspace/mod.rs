//! The workspace: tabs of widget layouts read from a plain file (agents and people edit it, the app
//! reloads), cards that open into detail screens, embedded terminals, toasts and an island.
//! Every change of place moves: tabs slide, screens push and pop, numbers ease, rows slide in.

pub mod cards;
pub mod config;
pub mod connect;
pub mod xauth;
pub mod media;
pub mod modules;
pub mod nowplay;
pub mod sessions;
pub mod quotes;
pub mod secret;
pub mod setup;
#[cfg(unix)]
pub mod term;
#[cfg(unix)]
pub mod voice;
#[cfg(unix)]
pub mod wrap;
pub mod cli;

use crate::island::Spring;
use crate::motion::{blit, ease_out, since, Tween};
use crate::theme::{self, Theme, PRESETS};
use crate::widgets::TextInput;
use crate::{fg, App, Buffer, Color, Ctx, Event, Frame, Key, KeyCode, Rect, Size, Style};
use cards::{card, hints, len, Sim};
use config::{Config, Height, Item, Row, Tab};
use std::time::SystemTime;

const ID_TAB: u32 = 100;
const ID_AGENT: u32 = 200;
const ID_WATCH: u32 = 300;
const ID_APP: u32 = 400;
const ID_TOAST: u32 = 500;
const ID_MEDIA: u32 = 600;
const ID_PLAY: u32 = 650;
const ID_SEEK: u32 = 651;
const ID_OPEN: u32 = 660;
const ID_CARD: u32 = 700;
const ID_BACK: u32 = 800;
const ID_RANGE: u32 = 900;
const ID_PAL: u32 = 1000;
/// The X card itself; its rows follow from ID_X + 1.
const ID_X: u32 = 1200;
/// The revenue card; invoice rows on the income screen follow from ID_INCOME + 1.
const ID_INCOME: u32 = 1300;
/// The start card's rows.
const ID_START: u32 = 1500;
/// The card gallery's rows (up to 400).
const ID_GAL: u32 = 1600;
/// Script modules: 32 ids per card on the desk, the card itself then its link rows.
const ID_MOD: u32 = 2000;

const KINDS: [&str; 9] = ["media", "apps", "agents", "watch", "revenue", "x", "term", "player", "start"];
const MODES: [&str; 4] = ["video", "lyrics", "split", "disc"];

#[derive(Clone, Copy, PartialEq, Debug)]
enum View {
    Desk,
    Stock(usize),
    Agent(usize),
    Player,
    /// What another app is playing, large: cover, title, controls.
    Now,
    /// Income at full size, with every invoice.
    Income,
    /// A setup step running in a terminal pane: index into setup::ITEMS.
    Setup(usize),
    /// `halftone connect` for a module card, running in a terminal pane (see `Workspace::conn`).
    Connect,
    /// Every module, to add as a card.
    Gallery,
}

/// What a row of the start card does.
#[derive(Clone)]
enum StartAct {
    Theme,
    Add,
    /// The module whose `connect:` line asks for a secret.
    Connect(String),
    Setup(&'static str),
}

/// The built-in setup steps the start card offers when their card is on the desk.
const STEPS: [&str; 3] = ["stripe", "x", "gmail"];

#[derive(Clone, Debug)]
enum Act {
    Cards,
    Tab(usize),
    Theme(usize),
    Mode(usize),
    Edit,
    Config,
    Help,
    Stock(usize),
    Income,
    Voice,
    NewTab(String),
    RenameTab(String),
    CloseTab,
    Quit,
}

pub struct Workspace {
    cfg: Config,
    mtime: Option<SystemTime>,
    note: Option<(String, f64)>,
    tab: usize,
    line_x: Tween,
    line_w: Tween,
    stack: Vec<View>,
    trans: Option<(f64, i32)>,
    last: Option<Buffer>,
    snap: Option<Buffer>,
    sim: Sim,
    next_beat: f64,
    deck: media::Deck,
    #[cfg(unix)]
    terms: Vec<(String, term::TermPane)>,
    /// Cards on screen this frame, for focus, clicks and the editor.
    cells: Vec<(Rect, usize, usize)>,
    focus: usize,
    typing: bool,
    sel: [usize; 4],
    /// Which `STEPS` are linked, refreshed with the config.
    checks: Vec<bool>,
    /// (module, shell command) of the connect view in progress.
    conn: Option<(String, String)>,
    /// The gallery: (name, about, group) of every module, and the selected one.
    gal_items: Vec<(String, String, String)>,
    gal: usize,
    /// Modules seen asking to connect, so the start card can tick them off once they stop.
    seen: Vec<String>,
    help: bool,
    sel_y: Tween,
    toasts: crate::toast::Toasts,
    island: Spring,
    island_msg: Option<(String, Color, f64)>,
    edit: bool,
    picker: Option<usize>,
    palette: Option<(TextInput, usize)>,
    #[cfg(unix)]
    voice: Option<voice::Voice>,
    range: usize,
    morph: Tween,
    prev_series: Vec<f32>,
    mode: usize,
    scroll: f32,
    hud_at: f64,
    theme_from: Theme,
    theme_to: usize,
    theme_at: f64,
    t: f64,
    dt: f32,
    demo: bool,
    inbox_seen: u64,
    polled: f64,
    feeds: connect::Feeds,
    now: nowplay::Monitor,
    /// Your real Claude Code and Codex sessions; none in recordings, which keep the sample rows.
    sessions: Option<std::sync::mpsc::Receiver<Vec<sessions::Session>>>,
    /// Live prices for the watchlist; none in recordings.
    quotes: Option<std::sync::mpsc::Receiver<(String, usize, quotes::Quote)>>,
    mods: modules::Mods,
    /// (hit id, url or command) of the module cards and links drawn this frame.
    mod_hits: Vec<(u32, String)>,
}

/// Desktop apps the inbox reads without setup, by their Dock badge: (service, app bundle).
const BADGE_APPS: [(&str, &str); 2] = [("discord", "/Applications/Discord.app"), ("gmail", "/System/Applications/Mail.app")];

/// The desk file's `connect` lines, plus Discord and Mail when they are installed and the file
/// says nothing about them: linking those needs no token, only the app.
fn connects(cfg: &Config, demo: bool) -> Vec<(String, Vec<String>)> {
    let mut v = cfg.connects.clone();
    if !demo && cfg!(target_os = "macos") {
        for (svc, app) in BADGE_APPS {
            if !v.iter().any(|(s, _)| s == svc) && std::path::Path::new(app).exists() {
                v.push((svc.into(), vec!["@app".into()]));
            }
        }
    }
    v
}

/// The inbox row or card a `connect` service feeds.
fn service_app(s: &str) -> &'static str {
    match s { "gmail" => "Gmail", "discord" => "Discord", "stripe" => "Stripe", _ => "X" }
}

pub(crate) fn theme_index(name: &str) -> usize {
    PRESETS.iter().position(|t| t.name == name).unwrap_or(0)
}

pub fn inbox() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache/halftone/inbox")
}

impl Workspace {
    pub fn new() -> Workspace {
        let fresh = !config::path().exists();
        modules::install();
        let (cfg, mut note) = match config::load() {
            Ok(c) => (c, None),
            Err(e) => (config::parse(config::DEFAULT).unwrap(), Some((format!("config: {e}"), 0.0))),
        };
        #[cfg(unix)]
        crate::term::keep_ctrl_c(true);
        let ti = theme_index(&cfg.theme);
        theme::set(PRESETS[ti]);
        let deck = media::Deck::new(&cfg.media);
        let demo = std::env::var("HALFTONE_DEMO").is_ok();
        let feeds = connect::Feeds::start(&connects(&cfg, demo));
        // Recordings show a fixed track (a CC BY one) instead of what this machine plays.
        let now = nowplay::Monitor::start(demo.then(|| nowplay::Track {
            app: "Music".into(), title: "Night Owl".into(), artist: "Broke For Free".into(), album: "Directionless EP".into(),
            pos: 12.0, dur: 194.0, playing: true, art: config::expand("~/.cache/halftone/media/nightowl.m4a"),
        }));
        let inbox_seen = std::fs::metadata(inbox()).map(|m| m.len()).unwrap_or(0);
        if fresh && note.is_none() {
            note = Some(("new desk · press a to add cards".into(), 0.0));
        }
        let mut w = Workspace {
            mtime: std::fs::metadata(config::path()).and_then(|m| m.modified()).ok(),
            cfg, note, tab: 0, line_x: Tween::new(0.0, 0.3), line_w: Tween::new(0.0, 0.3),
            stack: vec![View::Desk], trans: None, last: None, snap: None,
            sim: Sim::new(), next_beat: 1.0, deck,
            #[cfg(unix)]
            terms: vec![],
            cells: vec![], focus: 0, typing: false, sel: [0; 4], checks: vec![], conn: None, gal_items: vec![], gal: 0, seen: vec![], help: false, sel_y: Tween::new(0.0, 0.18),
            toasts: crate::toast::Toasts::new(), island: Spring::new(0.0), island_msg: None,
            edit: false, picker: None, palette: None,
            #[cfg(unix)]
            voice: None,
            range: 0, morph: Tween::new(1.0, 0.45), prev_series: vec![], mode: 2, scroll: 0.0, hud_at: 0.0,
            theme_from: PRESETS[ti], theme_to: ti, theme_at: -9.0,
            t: 0.0, dt: 0.016, demo: std::env::var("HALFTONE_DEMO").is_ok(), inbox_seen, polled: 0.0, feeds, now,
            sessions: (!demo).then(|| sessions::watch(6)),
            quotes: None,
            mods: modules::Mods::new(), mod_hits: vec![],
        };
        let live = connects(&w.cfg, w.demo).iter().map(|(s, _)| service_app(s)).collect();
        w.sim.quiet = !w.demo;
        w.sim.set_live(live);
        w.restart_quotes();
        w.refresh_checks();
        w
    }

    fn cur(&self) -> View {
        *self.stack.last().unwrap_or(&View::Desk)
    }

    /// Changes place with a slide: +1 pushes in from the right, -1 pops back.
    fn go(&mut self, dir: i32) {
        self.trans = Some((self.t, dir));
        self.snap = self.last.clone();
    }

    fn push(&mut self, v: View) {
        self.stack.push(v);
        self.typing = matches!(v, View::Agent(_));
        self.go(1);
    }

    fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
            self.typing = false;
            self.go(-1);
        }
    }

    fn switch_tab(&mut self, i: usize) {
        if i < self.cfg.tabs.len() && i != self.tab {
            let dir = if i > self.tab { 1 } else { -1 };
            self.tab = i;
            self.stack.truncate(1);
            (self.focus, self.typing) = (0, false);
            self.go(dir);
        }
    }

    fn set_theme(&mut self, i: usize) {
        self.theme_from = self.current_theme();
        self.theme_to = i % PRESETS.len();
        self.theme_at = self.t;
        self.cfg.theme = PRESETS[self.theme_to].name.into();
    }

    fn current_theme(&self) -> Theme {
        let k = since(self.t, self.theme_at, 0.6);
        self.theme_from.mix(&PRESETS[self.theme_to], k)
    }

    fn notify(&mut self, app: &str, tint: u32, title: &str, body: &str) {
        if !self.cfg.mods.iter().any(|m| m == "toasts") {
            return;
        }
        let agentish = matches!(app, "Claude" | "Codex");
        if agentish && self.cfg.mods.iter().any(|m| m == "island") {
            self.island_msg = Some((format!("{title} · {body}"), cards::tone(tint), self.t));
        } else {
            self.toasts.push(self.t, app, cards::tone(tint), title, body);
        }
    }

    fn item(&self, i: usize) -> Option<&Item> {
        let (_, row, col) = *self.cells.get(i)?;
        self.cfg.tabs.get(self.tab)?.rows.get(row)?.items.get(col)
    }

    fn focused_kind(&self) -> &str {
        self.item(self.focus).map_or("", |i| i.kind.as_str())
    }

    #[cfg(unix)]
    fn term(&mut self, key: &str) -> &mut term::TermPane {
        if let Some(i) = self.terms.iter().position(|(k, _)| k == key) {
            return &mut self.terms[i].1;
        }
        let cmd = key.split_once(':').map_or(key, |(_, c)| c);
        self.terms.push((key.to_string(), term::TermPane::new(cmd)));
        &mut self.terms.last_mut().unwrap().1
    }

    /// The focused terminal's key, if keys should go to one now.
    fn typing_into(&self) -> Option<String> {
        if !self.typing {
            return None;
        }
        match self.cur() {
            View::Agent(i) => Some(format!("agent{i}:{}", self.agent_cmd(i))),
            View::Setup(i) => Some(format!("setup{i}:{}", Self::setup_cmd(i))),
            View::Connect => self.conn.as_ref().map(|(_, c)| format!("connect:{c}")),
            View::Desk => self.item(self.focus).filter(|i| i.kind == "term").map(|i| format!("{}:{}", self.tab, i.args.join(" "))),
            _ => None,
        }
    }

    fn refresh_checks(&mut self) {
        let conn = connects(&self.cfg, self.demo);
        self.checks = STEPS.iter().map(|k| setup::linked(k, &self.cfg, &conn)).collect();
    }

    /// Every card kind on the desk, all tabs.
    fn kinds(&self) -> Vec<&str> {
        self.cfg.tabs.iter().flat_map(|t| &t.rows).flat_map(|r| &r.items).map(|i| i.kind.as_str()).collect()
    }

    /// The start card: theme, add cards, then one row per thing on the desk that wants an account.
    fn start_rows(&self) -> Vec<(String, String, bool, StartAct)> {
        let kinds = self.kinds();
        let mut v = vec![
            ("Pick a theme (t)".to_string(), if self.cfg.theme != "ink" { self.cfg.theme.clone() } else { "press t to try them".into() }, self.cfg.theme != "ink", StartAct::Theme),
            ("Add cards (a)".to_string(), "browse everything you can put here".into(), kinds.iter().any(|k| modules::exists(k)), StartAct::Add),
        ];
        let steps = [("revenue", 0, "Stripe", "paste a read-only key", "income and invoices are live"), ("x", 1, "X", "sign in", "signed in"), ("apps", 2, "Gmail", "add Google in Mail, or an app password", "unread mail is on the desk")];
        for (kind, i, name, todo, done) in steps {
            if kinds.contains(&kind) {
                let d = self.checks.get(i).copied().unwrap_or(false);
                v.push((format!("Connect {name}"), (if d { done } else { todo }).into(), d, StartAct::Setup(STEPS[i])));
            }
        }
        let mut seen: Vec<&str> = vec![];
        for k in kinds.into_iter().filter(|k| modules::exists(k)) {
            if seen.contains(&k) {
                continue;
            }
            seen.push(k);
            let out = self.mods.slots.get(k).and_then(|s| s.out.as_ref());
            match out.and_then(|o| o.connect.as_ref()) {
                Some((n, _, prompt)) => v.push((format!("Connect {}", modules::cap(n)), prompt.clone(), false, StartAct::Connect(k.into()))),
                None if self.seen.iter().any(|s| s == k) => v.push((format!("Connect {}", modules::cap(k)), "connected".into(), true, StartAct::Connect(k.into()))),
                None => {}
            }
        }
        v
    }

    /// Runs `halftone connect` for a module in a full view; when it exits the module runs again at once.
    fn open_connect(&mut self, module: &str, (name, url, prompt): (String, String, String)) {
        if self.demo {
            self.note = Some(("connecting is off in demo mode".into(), self.t));
            return;
        }
        #[cfg(unix)]
        {
            let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "halftone".into());
            let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
            let cmd = format!("{} connect {} {} {} && sleep 1 || {{ printf '\\n  Press Enter to go back... '; read _; }}", q(&exe), q(&name), q(&url), q(&prompt));
            self.terms.retain(|(k, _)| !k.starts_with("connect:"));
            self.terms.push((format!("connect:{cmd}"), term::TermPane::new(&cmd)));
            self.conn = Some((module.into(), cmd));
            self.push(View::Connect);
            self.typing = true;
        }
    }

    fn open_gallery(&mut self) {
        self.gal_items = modules::list().into_iter().map(|n| {
            let (about, group) = modules::meta(&std::fs::read_to_string(modules::path(&n)).unwrap_or_default());
            (n, about, if group.is_empty() { "Other".into() } else { group })
        }).collect();
        self.gal_items.sort_by(|a, b| ((a.2 == "Other"), &a.2, &a.0).cmp(&((b.2 == "Other"), &b.2, &b.0)));
        self.gal = self.gal.min(self.gal_items.len().saturating_sub(1));
        if self.cur() != View::Gallery {
            self.push(View::Gallery);
        }
    }

    /// Adds module `i` of the gallery to this tab: into the last row if it holds fewer than 3, else a new row.
    fn gal_add(&mut self, i: usize) {
        let Some((name, ..)) = self.gal_items.get(i).cloned() else { return };
        let Some(tab) = self.cfg.tabs.get_mut(self.tab) else { return };
        let item = Item { kind: name.clone(), args: vec![], weight: 1 };
        match tab.rows.last_mut() {
            Some(r) if r.items.len() < 3 => r.items.push(item),
            _ => tab.rows.push(Row { h: Height::Fill(1), items: vec![item] }),
        }
        let tab = tab.name.clone();
        self.save();
        self.note = Some((format!("added {name} to {tab}"), self.t));
    }

    fn setup_cmd(i: usize) -> String {
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "halftone".into());
        // The pause keeps the step's last words readable before the desk comes back.
        format!("'{exe}' setup {}; printf '\n  Press Enter to go back... '; read _", setup::ITEMS[i].0)
    }

    /// Runs `halftone setup <step>` in a full view; when it exits the desk comes back and reloads.
    fn open_setup(&mut self, step: &str) {
        let Some(i) = setup::ITEMS.iter().position(|(n, _)| *n == step) else { return };
        if self.demo {
            self.note = Some(("setup is off in demo mode".into(), self.t));
            return;
        }
        #[cfg(unix)]
        {
            let cmd = Self::setup_cmd(i);
            self.terms.retain(|(k, _)| !k.starts_with(&format!("setup{i}:")));
            self.terms.push((format!("setup{i}:{cmd}"), term::TermPane::new(&cmd)));
            self.push(View::Setup(i));
            self.typing = true;
        }
    }

    /// Acts on row `i` of the start card: a setup step, the theme, the gallery, or hiding the card.
    fn start_act(&mut self, i: usize) {
        match self.start_rows().into_iter().nth(i).map(|r| r.3) {
            Some(StartAct::Theme) => {
                self.set_theme(self.theme_to + 1);
                self.save();
            }
            Some(StartAct::Add) => self.open_gallery(),
            Some(StartAct::Connect(m)) => {
                if let Some(c) = self.mods.slots.get(&m).and_then(|s| s.out.as_ref()).and_then(|o| o.connect.clone()) {
                    self.open_connect(&m, c);
                }
            }
            Some(StartAct::Setup(step)) => self.open_setup(step),
            None => {
                for t in &mut self.cfg.tabs {
                    t.rows.iter_mut().for_each(|r| r.items.retain(|it| it.kind != "start"));
                    t.rows.retain(|r| !r.items.is_empty());
                }
                self.focus = 0;
                self.save();
            }
        }
    }

    fn restart_quotes(&mut self) {
        self.sim.set_watch(&self.cfg.watch, !self.demo);
        let syms = self.sim.tickers.iter().map(|t| t.sym.clone()).collect();
        self.quotes = (!self.demo).then(|| quotes::watch(syms));
    }

    fn agent_cmd(&self, i: usize) -> String {
        if let Ok(c) = std::env::var("HALFTONE_AGENT_CMD") {
            return c;
        }
        match self.sim.agents.get(i) {
            Some(a) if !a.cmd.is_empty() => a.cmd.clone(),
            Some(a) if a.tool == "codex" => "codex".into(),
            _ => self.cfg.agent.clone(),
        }
    }

    fn open_focused(&mut self) {
        match self.focused_kind() {
            "media" => self.push(View::Now),
            "player" => self.push(View::Player),
            "agents" => self.push(View::Agent(self.sel[0])),
            "watch" => self.open_stock(self.sel[1]),
            "apps" => self.launch(self.sel[2]),
            "x" | "news" if self.unlinked("X") => self.open_setup("x"),
            "x" | "news" => self.open_url("https://x.com/home"),
            "revenue" if self.unlinked("Stripe") => self.open_setup("stripe"),
            "revenue" => self.push(View::Income),
            "start" => self.start_act(self.sel[3]),
            "term" => self.typing = true,
            _ => self.open_mod(ID_MOD + self.focus as u32 * 32),
        }
    }

    fn open_mod(&mut self, id: u32) {
        if let Some((_, url)) = self.mod_hits.iter().find(|(i, _)| *i == id).cloned() {
            // cards::module marks a `connect:` card with \u{1}name TAB url TAB prompt.
            if let Some(c) = url.strip_prefix('\u{1}') {
                let mut p = c.splitn(3, '\t').map(String::from);
                if let (Some(n), Some(u), Some(q), Some(m)) = (p.next(), p.next(), p.next(), self.item(self.focus).map(|i| i.kind.clone())) {
                    self.open_connect(&m, (n, u, q));
                }
            } else if !self.demo {
                modules::open(&url);
            }
        }
    }

    /// A service with no `connect` line, outside demo mode: its card offers setup instead.
    fn unlinked(&self, svc: &str) -> bool {
        !self.demo && !self.sim.live.contains(&svc)
    }

    fn open_stock(&mut self, i: usize) {
        if i < self.sim.tickers.len() {
            self.range = 0;
            self.prev_series = cards::series(&self.sim.tickers[i], 0);
            self.morph.snap(1.0);
            self.push(View::Stock(i));
        }
    }

    fn set_range(&mut self, r: usize) {
        if let View::Stock(i) = self.cur() {
            let t = &self.sim.tickers[i];
            self.prev_series = cards::series(t, self.range);
            self.range = r.min(cards::RANGES.len() - 1);
            self.morph.snap(0.0);
            self.morph.set(1.0);
        }
    }

    fn open_url(&self, url: &str) {
        if !self.demo {
            let cmd = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
            let _ = std::process::Command::new(cmd).arg(url).spawn();
        }
    }

    fn launch(&mut self, i: usize) {
        if let Some(a) = self.sim.apps.get(i) {
            if self.unlinked(a.name) {
                return self.open_setup(&a.name.to_lowercase());
            }
            if !self.demo {
                // A row read from a desktop app opens that app; otherwise the web.
                let svc = a.name.to_lowercase();
                let by_app = connects(&self.cfg, false).iter().any(|(s, args)| *s == svc && args.first().map(String::as_str) == Some("@app"));
                let app = BADGE_APPS.iter().find(|(s, _)| *s == svc).map(|(_, p)| *p);
                match app.filter(|_| by_app && cfg!(target_os = "macos")) {
                    Some(path) => _ = std::process::Command::new("open").arg("-a").arg(path).spawn(),
                    None => {
                        let cmd = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
                        let _ = std::process::Command::new(cmd).arg(a.url).spawn();
                    }
                }
            }
            let a = &mut self.sim.apps[i];
            a.unread.set(0.0);
        }
    }

    fn reload(&mut self) {
        let m = std::fs::metadata(config::path()).and_then(|m| m.modified()).ok();
        if m == self.mtime {
            return;
        }
        self.mtime = m;
        match config::load() {
            Ok(c) => {
                if c.theme != self.cfg.theme {
                    self.set_theme(theme_index(&c.theme));
                }
                if c.connects != self.cfg.connects {
                    let all = connects(&c, self.demo);
                    self.feeds = connect::Feeds::start(&all);
                    self.sim.set_live(all.iter().map(|(s, _)| service_app(s)).collect());
                }
                if c.media != self.cfg.media {
                    self.deck = media::Deck::new(&c.media);
                }
                let rewatch = c.watch != self.cfg.watch;
                self.cfg = c;
                if rewatch {
                    self.restart_quotes();
                }
                self.tab = self.tab.min(self.cfg.tabs.len() - 1);
                self.note = Some(("layout reloaded".into(), self.t));
            }
            Err(e) => self.note = Some((format!("config: {e}"), self.t)),
        }
    }

    fn save(&mut self) {
        match config::save(&self.cfg) {
            Ok(()) => {
                self.mtime = std::fs::metadata(config::path()).and_then(|m| m.modified()).ok();
                self.note = Some((format!("layout saved to {}", config::path().file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())), self.t));
            }
            Err(e) => self.note = Some((e, self.t)),
        }
    }

    /// `halftone notify` appends `app<TAB>title<TAB>body` lines; new ones become toasts.
    fn poll_inbox(&mut self) {
        let p = inbox();
        let Ok(m) = std::fs::metadata(&p) else { return };
        if m.len() <= self.inbox_seen {
            self.inbox_seen = m.len();
            return;
        }
        let Ok(text) = std::fs::read(&p) else { return };
        let fresh = String::from_utf8_lossy(&text[self.inbox_seen as usize..]).to_string();
        self.inbox_seen = m.len();
        for line in fresh.lines() {
            let mut p = line.splitn(3, '\t');
            let (app, title, body) = (p.next().unwrap_or("halftone"), p.next().unwrap_or(""), p.next().unwrap_or(""));
            let tint = app_tint(app);
            self.notify(app, tint, title, body);
        }
    }

    fn feed(&mut self, u: connect::Update) {
        match u {
            connect::Update::Inbox { app, unread, preview, new } => {
                self.sim.errs.retain(|(s, _)| *s != app);
                if let Some(a) = self.sim.apps.iter_mut().find(|a| a.name == app) {
                    a.unread.set(unread as f32);
                    a.preview = preview;
                    if new.is_some() {
                        a.at = self.t;
                    }
                }
                if let Some((title, body)) = new {
                    self.notify(app, app_tint(app), &title, &body);
                }
            }
            connect::Update::Invoices(v) => {
                self.sim.errs.retain(|(s, _)| *s != "Stripe");
                self.sim.set_invoices(v);
            }
            connect::Update::Posts(p) => {
                self.sim.errs.retain(|(s, _)| *s != "X");
                self.sim.posts = p;
            }
            connect::Update::Error(s, e) => {
                self.sim.errs.retain(|(n, _)| *n != s);
                self.sim.errs.push((s, e));
            }
        }
    }

    fn act(&mut self, a: Act, ctx: &mut Ctx) {
        match a {
            Act::Tab(i) => self.switch_tab(i),
            Act::Theme(i) => {
                self.set_theme(i);
                self.save();
            }
            Act::Mode(m) => {
                self.mode = m;
                if self.cur() != View::Player {
                    self.push(View::Player);
                }
            }
            Act::Edit => self.toggle_edit(),
            Act::Config => {
                let ed = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
                let cmd = format!("{ed} {}", config::path().display());
                #[cfg(unix)]
                {
                    self.terms.retain(|(k, _)| !k.starts_with("agent99:"));
                    self.terms.push((format!("agent99:{cmd}"), term::TermPane::new(&cmd)));
                }
                self.push(View::Agent(99));
            }
            Act::Help => self.help = true,
            Act::Cards => self.open_gallery(),
            Act::Stock(i) => self.open_stock(i),
            Act::Income => self.push(View::Income),
            Act::Voice => self.toggle_voice(),
            Act::NewTab(name) => {
                let first = Item { kind: "agents".into(), args: vec![], weight: 1 };
                self.cfg.tabs.push(Tab { name, rows: vec![Row { h: Height::Fill(1), items: vec![first] }] });
                self.switch_tab(self.cfg.tabs.len() - 1);
                // Straight into the editor: `a` adds cards, `x` removes them, `e` saves.
                self.edit = true;
            }
            Act::RenameTab(name) => {
                self.cfg.tabs[self.tab].name = name;
                self.save();
            }
            Act::CloseTab => {
                if self.cfg.tabs.len() > 1 {
                    self.cfg.tabs.remove(self.tab);
                    self.switch_tab(self.tab.saturating_sub(1));
                    self.tab = self.tab.min(self.cfg.tabs.len() - 1);
                    self.save();
                }
            }
            Act::Quit => ctx.quit(),
        }
    }

    fn commands(&self) -> Vec<(String, Act)> {
        let mut v: Vec<(String, Act)> = vec![];
        v.extend(self.cfg.tabs.iter().enumerate().map(|(i, t)| (format!("Go to {}", t.name), Act::Tab(i))));
        if self.cfg.tabs.len() > 1 {
            v.push(("Close this tab".into(), Act::CloseTab));
        }
        for (i, t) in self.sim.tickers.iter().enumerate() {
            v.push((format!("Open {} chart", t.sym), Act::Stock(i)));
        }
        v.push(("Open income and invoices".into(), Act::Income));
        for (i, m) in MODES.iter().enumerate() {
            v.push((format!("Player: {m}"), Act::Mode(i)));
        }
        for (i, t) in PRESETS.iter().enumerate() {
            v.push((format!("Theme: {}", t.name), Act::Theme(i)));
        }
        v.push(("Add a card…".into(), Act::Cards));
        v.push(("Help".into(), Act::Help));
        v.push(("Edit layout".into(), Act::Edit));
        v.push(("Edit config file".into(), Act::Config));
        v.push(("Voice: talk to the focused terminal".into(), Act::Voice));
        v.push(("Quit".into(), Act::Quit));
        // What you type can name a tab: "New tab “Trading”". Last, so real matches come first.
        let q = self.palette.as_ref().map_or(String::new(), |p| p.0.value.trim().to_string());
        if !q.is_empty() {
            v.push((format!("New tab “{q}”"), Act::NewTab(q.clone())));
            v.push((format!("Rename this tab to “{q}”"), Act::RenameTab(q)));
        }
        v
    }

    fn filtered(&self) -> Vec<(String, Act)> {
        let q = self.palette.as_ref().map_or(String::new(), |p| p.0.value.to_lowercase());
        self.commands().into_iter().filter(|(n, _)| {
            let n = n.to_lowercase();
            let mut it = n.chars();
            q.chars().all(|c| it.any(|d| d == c))
        }).collect()
    }

    fn toggle_edit(&mut self) {
        self.edit = !self.edit;
        self.picker = None;
        if !self.edit {
            self.save();
        }
    }

    #[cfg(unix)]
    fn toggle_voice(&mut self) {
        match self.voice.take() {
            None => match voice::Voice::start() {
                Ok(v) => self.voice = Some(v),
                Err(e) => self.note = Some((e, self.t)),
            },
            Some(v) => {
                let target = self.typing_into();
                match v.finish() {
                    Ok(text) if !text.is_empty() => {
                        match target {
                            Some(k) => self.term(&k).paste(&text),
                            None => self.note = Some((format!("heard: {text}"), self.t)),
                        }
                    }
                    Ok(_) => self.note = Some(("heard nothing".into(), self.t)),
                    Err(e) => self.note = Some((e, self.t)),
                }
            }
        }
    }

    #[cfg(not(unix))]
    fn toggle_voice(&mut self) {}

    /// Editor moves: the selected card is `self.focus`, in reading order.
    fn edit_key(&mut self, k: &Key) -> bool {
        let Some(&(_, row, col)) = self.cells.get(self.focus) else { return false };
        let tab = &mut self.cfg.tabs[self.tab];
        if let Some(p) = self.picker {
            match k.code {
                KeyCode::Up => self.picker = Some(p.saturating_sub(1)),
                KeyCode::Down => self.picker = Some((p + 1).min(KINDS.len() - 1)),
                KeyCode::Enter => {
                    let args = match KINDS[p] { "term" => vec!["shell".to_string()], "player" => vec!["split".to_string()], _ => vec![] };
                    tab.rows[row].items.insert(col + 1, Item { kind: KINDS[p].into(), args, weight: 1 });
                    self.picker = None;
                    self.focus += 1;
                }
                KeyCode::Esc => self.picker = None,
                _ => {}
            }
            return true;
        }
        let items = &mut tab.rows[row].items;
        match k.code {
            KeyCode::Char('h') | KeyCode::Char('H') if col > 0 => {
                items.swap(col, col - 1);
                self.focus -= 1;
            }
            KeyCode::Char('l') | KeyCode::Char('L') if col + 1 < items.len() => {
                items.swap(col, col + 1);
                self.focus += 1;
            }
            KeyCode::Char('+') | KeyCode::Char('=') => items[col].weight = (items[col].weight + 1).min(9),
            KeyCode::Char('-') => items[col].weight = items[col].weight.saturating_sub(1).max(1),
            KeyCode::Char('k') | KeyCode::Char('K') if row > 0 => {
                let it = items.remove(col);
                tab.rows[row - 1].items.push(it);
                tab.rows.retain(|r| !r.items.is_empty());
            }
            KeyCode::Char('j') | KeyCode::Char('J') => {
                let it = items.remove(col);
                if row + 1 < tab.rows.len() {
                    tab.rows[row + 1].items.insert(0, it);
                } else {
                    tab.rows.push(config::Row { h: Height::Fill(1), items: vec![it] });
                }
                tab.rows.retain(|r| !r.items.is_empty());
            }
            KeyCode::Char('x') | KeyCode::Delete => {
                items.remove(col);
                tab.rows.retain(|r| !r.items.is_empty());
                self.focus = self.focus.saturating_sub(1);
            }
            KeyCode::Char('a') => self.picker = Some(0),
            KeyCode::Char('n') => tab.rows.insert(row + 1, config::Row { h: Height::Fill(1), items: vec![Item { kind: "news".into(), args: vec![], weight: 1 }] }),
            KeyCode::Char('[') => {
                tab.rows[row].h = match tab.rows[row].h { Height::Len(n) => Height::Len(n.saturating_sub(1).max(4)), Height::Fill(w) => Height::Fill(w.saturating_sub(1).max(1)) };
            }
            KeyCode::Char(']') => {
                tab.rows[row].h = match tab.rows[row].h { Height::Len(n) => Height::Len(n + 1), Height::Fill(w) => Height::Fill(w + 1) };
            }
            KeyCode::Left | KeyCode::BackTab => self.focus = self.focus.saturating_sub(1),
            KeyCode::Right | KeyCode::Tab => self.focus = (self.focus + 1).min(self.cells.len().saturating_sub(1)),
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('e') | KeyCode::Char('s') => self.toggle_edit(),
            _ => return false,
        }
        true
    }

    fn key(&mut self, k: Key, ctx: &mut Ctx) {
        // The palette and the voice toggle sit above everything, terminals included.
        if k.ctrl && k.code == KeyCode::Char('k') {
            self.palette = if self.palette.is_some() { None } else { Some((TextInput::default(), 0)) };
            return;
        }
        if k.ctrl && k.code == KeyCode::Char(' ') {
            self.toggle_voice();
            return;
        }
        if let Some((input, sel)) = &mut self.palette {
            match k.code {
                KeyCode::Esc => self.palette = None,
                KeyCode::Up => *sel = sel.saturating_sub(1),
                KeyCode::Down => *sel += 1,
                KeyCode::Enter => {
                    let i = *sel;
                    let list = self.filtered();
                    self.palette = None;
                    if let Some((_, a)) = list.into_iter().nth(i) {
                        self.act(a, ctx);
                    }
                }
                _ => {
                    input.handle(&Event::Key(k));
                    *sel = 0;
                }
            }
            return;
        }
        if self.help {
            if matches!(k.code, KeyCode::Esc | KeyCode::Char('?')) {
                self.help = false;
            }
            return;
        }
        #[cfg(unix)]
        if let Some(t) = self.typing_into() {
            if k.ctrl && k.code == KeyCode::Char(']') {
                if matches!(self.cur(), View::Agent(_) | View::Setup(_) | View::Connect) { self.pop() } else { self.typing = false }
                return;
            }
            for (combo, action) in self.cfg.keys.clone() {
                if combo_matches(&combo, &k) {
                    if action == "voice" {
                        self.toggle_voice();
                        return;
                    }
                    if let Some(text) = action.strip_prefix("send ") {
                        self.term(&t).send(unescape(text).as_bytes());
                        return;
                    }
                }
            }
            self.term(&t).key(&k);
            return;
        }
        if self.edit && self.edit_key(&k) {
            return;
        }
        for (combo, action) in self.cfg.keys.clone() {
            if combo_matches(&combo, &k) {
                self.run(&action, ctx);
                return;
            }
        }
        if k.ctrl && k.code == KeyCode::Char('c') {
            ctx.quit();
            return;
        }
        if k.ctrl || k.alt {
            return;
        }
        let view = self.cur();
        match k.code {
            KeyCode::Char('q') => ctx.quit(),
            KeyCode::Esc | KeyCode::Backspace => self.pop(),
            KeyCode::Char(']') => self.switch_tab((self.tab + 1) % self.cfg.tabs.len()),
            KeyCode::Char('[') => self.switch_tab((self.tab + self.cfg.tabs.len() - 1) % self.cfg.tabs.len()),
            KeyCode::Char(c @ '1'..='9') => self.switch_tab(c as usize - '1' as usize),
            KeyCode::Char('t') => self.set_theme(self.theme_to + 1),
            KeyCode::Char('e') if view == View::Desk => self.toggle_edit(),
            KeyCode::Char('a') if view == View::Desk => self.open_gallery(),
            KeyCode::Left if view == View::Gallery => self.pop(),
            KeyCode::Up if view == View::Gallery => self.gal = self.gal.saturating_sub(1),
            KeyCode::Down if view == View::Gallery => self.gal = (self.gal + 1).min(self.gal_items.len().saturating_sub(1)),
            KeyCode::Enter if view == View::Gallery => self.gal_add(self.gal),
            KeyCode::Char('?') => self.help = true,
            KeyCode::Char(':') => self.palette = Some((TextInput::default(), 0)),
            // Halftone's own player answers in its view; everywhere else the keys go to the app playing.
            KeyCode::Char(' ') if view == View::Player => _ = self.deck.pb().map(|p| p.toggle()),
            KeyCode::Char('n') if view == View::Player => self.deck.open((self.deck.active + 1) % self.deck.items.len().max(1)),
            KeyCode::Char(' ') => self.now.control("toggle"),
            KeyCode::Char('n') => self.now.control("next"),
            KeyCode::Char('p') => self.now.control("previous"),
            KeyCode::Char('m') => self.mode = (self.mode + 1) % MODES.len(),
            KeyCode::Tab => self.focus = (self.focus + 1) % self.cells.len().max(1),
            KeyCode::BackTab => self.focus = (self.focus + self.cells.len().max(1) - 1) % self.cells.len().max(1),
            KeyCode::Enter if view == View::Desk => self.open_focused(),
            KeyCode::Left if matches!(view, View::Stock(_)) => self.set_range(self.range.saturating_sub(1)),
            KeyCode::Right if matches!(view, View::Stock(_)) => self.set_range(self.range + 1),
            KeyCode::Up | KeyCode::Down if view == View::Desk => {
                let down = k.code == KeyCode::Down;
                let (slot, n) = match self.focused_kind() {
                    "agents" => (0, self.sim.agents.len()),
                    "watch" => (1, self.sim.tickers.len()),
                    "apps" => (2, self.sim.apps.len()),
                    "start" => (3, self.start_rows().len() + 1),
                    _ => return,
                };
                let s = &mut self.sel[slot];
                *s = if down { (*s + 1).min(n.saturating_sub(1)) } else { s.saturating_sub(1) };
            }
            _ => {}
        }
    }

    fn run(&mut self, action: &str, ctx: &mut Ctx) {
        let (verb, arg) = action.split_once(' ').unwrap_or((action, ""));
        match verb {
            "tab" => {
                if let Some(i) = self.cfg.tabs.iter().position(|t| t.name.eq_ignore_ascii_case(arg)) {
                    self.switch_tab(i);
                }
            }
            "theme" => self.set_theme(theme_index(arg)),
            "run" => _ = std::process::Command::new("sh").args(["-c", arg]).spawn(),
            "quit" => ctx.quit(),
            "voice" => self.toggle_voice(),
            _ => {}
        }
    }
}

pub(crate) fn app_tint(app: &str) -> u32 {
    match app.to_lowercase().as_str() {
        "discord" => 0x5865f2,
        "gmail" | "mail" => 0xea4335,
        "instagram" => 0xe1306c,
        "x" => 0xe7e9ea,
        "claude" => 0xd97757,
        "codex" => 0x10a37f,
        "chrome" | "browser" | "safari" => 0x8ab4f8,
        "slack" => 0x4a154b,
        _ => 0x8a93a0,
    }
}

pub(crate) fn combo_matches(combo: &str, k: &Key) -> bool {
    let mut ctrl = false;
    let mut alt = false;
    let mut key = "";
    for part in combo.split('+') {
        match part {
            "ctrl" => ctrl = true,
            "alt" | "opt" => alt = true,
            p => key = p,
        }
    }
    let code = match key {
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "space" => KeyCode::Char(' '),
        s if s.chars().count() == 1 => KeyCode::Char(s.chars().next().unwrap()),
        _ => return false,
    };
    k.ctrl == ctrl && k.alt == alt && k.code == code
}

pub(crate) fn unescape(s: &str) -> String {
    s.replace("\\r", "\r").replace("\\n", "\n").replace("\\t", "\t")
}

impl Default for Workspace {
    fn default() -> Self {
        Workspace::new()
    }
}

impl App for Workspace {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match *ev {
            Event::Tick(dt) => {
                self.t += dt;
                self.dt = dt as f32;
                self.sim.tick(dt);
                self.mods.drain();
                self.deck.tick(dt);
                self.now.tick(dt);
                self.sel_y.step(self.dt);
                self.line_x.step(self.dt);
                self.line_w.step(self.dt);
                self.morph.step(self.dt);
                self.island.step(self.dt);
                #[cfg(unix)]
                for (_, t) in &mut self.terms {
                    t.pump(dt);
                }
                #[cfg(unix)]
                if let Some(v) = &mut self.voice {
                    v.tick();
                }
                if self.t >= self.next_beat {
                    self.next_beat += 1.0;
                    if self.demo {
                        if let Some((app, tint, title, body)) = self.sim.beat(self.t) {
                            self.notify(app, tint, &title, &body);
                        }
                    } else {
                        self.sim.beat(self.t);
                    }
                }
                while let Ok(u) = self.feeds.rx.try_recv() {
                    self.feed(u);
                }
                while let Some((sym, range, q)) = self.quotes.as_ref().and_then(|rx| rx.try_recv().ok()) {
                    self.sim.quote(&sym, range, q);
                }
                while let Some(v) = self.sessions.as_ref().and_then(|rx| rx.try_recv().ok()) {
                    self.sim.agents = v.into_iter().map(|s| cards::Agent {
                        repo: s.repo(), cmd: s.command(), tool: s.tool.clone(),
                        task: s.task.clone(), doing: s.doing.clone(),
                        state: if s.idle < 60.0 { cards::State::Working } else { cards::State::Done },
                        secs: s.idle, ctx: 0.0, changed: -9.0,
                    }).collect();
                }
                #[cfg(unix)]
                if let View::Setup(i) = self.cur() {
                    let key = format!("setup{i}:{}", Self::setup_cmd(i));
                    if self.terms.iter().any(|(k, t)| *k == key && t.exited) {
                        self.terms.retain(|(k, _)| *k != key);
                        self.pop();
                        self.mtime = None;
                    }
                }
                #[cfg(unix)]
                if let (View::Connect, Some((m, c))) = (self.cur(), self.conn.clone()) {
                    let key = format!("connect:{c}");
                    if self.terms.iter().any(|(k, t)| *k == key && t.exited) {
                        self.terms.retain(|(k, _)| *k != key);
                        self.pop();
                        self.conn = None;
                        self.mods.refresh(&m);
                    }
                }
                if self.t - self.polled > 0.5 {
                    for (k, s) in &self.mods.slots {
                        if s.out.as_ref().is_some_and(|o| o.connect.is_some()) && !self.seen.contains(k) {
                            self.seen.push(k.clone());
                        }
                    }
                    self.polled = self.t;
                    self.reload();
                    self.refresh_checks();
                    self.poll_inbox();
                }
                let th = self.current_theme();
                theme::set(th);
            }
            Event::Key(k) => self.key(k, ctx),
            Event::Click { id, x, rect, .. } => {
                self.hud_at = self.t;
                match id {
                    ID_BACK => self.pop(),
                    _ if (ID_TAB..ID_TAB + 50).contains(&id) => self.switch_tab((id - ID_TAB) as usize),
                    _ if (ID_AGENT..ID_AGENT + 50).contains(&id) => {
                        self.sel[0] = (id - ID_AGENT) as usize;
                        self.push(View::Agent(self.sel[0]));
                    }
                    _ if (ID_WATCH..ID_WATCH + 50).contains(&id) => {
                        self.sel[1] = (id - ID_WATCH) as usize;
                        self.open_stock(self.sel[1]);
                    }
                    _ if (ID_APP..ID_APP + 50).contains(&id) => self.launch((id - ID_APP) as usize),
                    _ if (ID_TOAST..ID_TOAST + 10).contains(&id) => self.toasts.dismiss((id - ID_TOAST) as usize),
                    _ if (ID_MEDIA..ID_MEDIA + 20).contains(&id) => self.deck.open((id - ID_MEDIA) as usize),
                    ID_PLAY if self.cur() == View::Player => _ = self.deck.pb().map(|p| p.toggle()),
                    ID_PLAY => self.now.control("toggle"),
                    ID_OPEN if self.cur() == View::Desk => self.push(View::Now),
                    1100 => {
                        if let View::Stock(i) = self.cur() {
                            let sym = self.sim.tickers[i].sym.clone();
                            self.open_url(&format!("https://x.com/search?q=%24{sym}&src=cashtag_click"));
                        }
                    }
                    ID_SEEK if self.cur() != View::Player => {
                        let d = self.now.now.as_ref().map_or(0.0, |t| t.dur);
                        self.now.seek(x as f64 / rect.w.max(1) as f64 * d);
                    }
                    ID_SEEK => {
                        if let Some(pb) = self.deck.pb() {
                            let l = pb.len.unwrap_or(0.0);
                            let _ = pb.seek_to(x as f64 / rect.w.max(1) as f64 * l);
                        }
                    }
                    _ if (ID_CARD..ID_CARD + 50).contains(&id) => {
                        self.focus = (id - ID_CARD) as usize;
                        self.typing = self.focused_kind() == "term";
                    }
                    _ if (ID_RANGE..ID_RANGE + 5).contains(&id) => self.set_range((id - ID_RANGE) as usize),
                    ID_X if self.unlinked("X") => self.open_setup("x"),
                    ID_X => self.open_url("https://x.com/home"),
                    ID_INCOME if self.cur() == View::Desk && self.unlinked("Stripe") => self.open_setup("stripe"),
                    ID_INCOME if self.cur() == View::Desk => self.push(View::Income),
                    _ if (ID_START..ID_START + 20).contains(&id) => {
                        self.sel[3] = (id - ID_START) as usize;
                        self.start_act(self.sel[3]);
                    }
                    _ if (ID_INCOME + 1..ID_INCOME + 99).contains(&id) => {
                        if let Some(i) = self.sim.invoices.get((id - ID_INCOME - 1) as usize) {
                            self.open_url(&i.url.clone());
                        }
                    }
                    _ if (ID_X + 1..ID_X + 40).contains(&id) => {
                        if let Some((_, _, url)) = self.sim.posts.get((id - ID_X - 1) as usize) {
                            self.open_url(&url.clone());
                        }
                    }
                    _ if (ID_GAL..ID_GAL + 400).contains(&id) => {
                        self.gal = (id - ID_GAL) as usize;
                        self.gal_add(self.gal);
                    }
                    _ if id >= ID_MOD => {
                        self.focus = ((id - ID_MOD) / 32) as usize;
                        self.open_mod(id);
                    }
                    _ if (ID_PAL..ID_PAL + 30).contains(&id) => {
                        let list = self.filtered();
                        self.palette = None;
                        if let Some((_, a)) = list.into_iter().nth((id - ID_PAL) as usize) {
                            self.act(a, ctx);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn view(&mut self, f: &mut Frame) {
        let a = f.area;
        f.buf.fill(a, theme::bg());
        if a.w < 70 || a.h < 20 {
            f.buf.print(a.x + 1, a.y, "make the terminal a little bigger", fg(theme::muted()), a.w);
            return;
        }
        self.top_bar(f, Rect::new(a.x, a.y, a.w, 2));
        let body = Rect::new(a.x + 1, a.y + 3, a.w - 2, a.h - 5);
        match self.cur() {
            View::Desk => self.desk(f, body),
            View::Stock(i) => self.stock(f, body, i),
            View::Agent(i) => self.agent(f, body, i),
            View::Player => self.player(f, body),
            View::Now => self.now_view(f, body),
            View::Setup(i) => self.setup_view(f, body, i),
            View::Connect => self.connect_view(f, body),
            View::Gallery => self.gallery(f, body),
            View::Income => {
                self.back(f, body);
                if !self.demo && !self.sim.income_live {
                    f.buf.print(body.x + 10, body.y, "sample data · run  halftone setup stripe  for yours", fg(theme::faint()), body.w.saturating_sub(12));
                }
                let r = Rect::new(body.x, body.y + 2, body.w, body.h.saturating_sub(2));
                cards::income(f, r, &self.sim, ID_INCOME + 1);
            }
        }
        self.transition(f, Rect::new(a.x, a.y + 2, a.w, a.h - 3));
        self.last = Some(f.buf.clone());
        self.footer(f, Rect::new(a.x + 1, a.bottom() - 1, a.w - 2, 1));
        self.island_draw(f, a);
        // On the desk, toasts start under a short top row (the inbox) instead of covering it.
        let top = match (self.cur(), self.cfg.tabs.get(self.tab).and_then(|t| t.rows.first())) {
            (View::Desk, Some(Row { h: Height::Len(n), .. })) if *n <= 8 => n + 1,
            _ => 0,
        };
        let ta = Rect::new(a.x, a.y + 3 + top, a.w, a.h.saturating_sub(5 + top));
        let dt = self.dt;
        self.toasts.draw(f, ta, dt, ID_TOAST);
        #[cfg(unix)]
        self.voice_draw(f, a);
        if self.palette.is_some() {
            self.palette_draw(f, a);
        }
        if self.help {
            self.help_draw(f, a);
        }
    }
}

impl Workspace {
    fn transition(&mut self, f: &mut Frame, r: Rect) {
        let Some((at, dir)) = self.trans else { return };
        let k = ((self.t - at) / 0.34) as f32;
        if k >= 1.0 {
            self.trans = None;
            return;
        }
        // Pixel pictures cannot slide with the cells; they come back when the move settles.
        f.buf.images.clear();
        let cur = f.buf.clone();
        f.buf.fill(r, theme::bg());
        let bg = theme::bg();
        // Old screen leaves first, then the new one arrives: a skip, not a cross-dissolve.
        if k < 0.4 {
            if let Some(old) = &self.snap {
                let e = ease_out(k / 0.4);
                blit(f.buf, old, r, -(dir * (e * 8.0) as i32), e, bg);
            }
        } else {
            let e = ease_out((k - 0.4) / 0.6);
            blit(f.buf, &cur, r, dir * ((1.0 - e) * 12.0) as i32, (1.0 - e) * 0.85, bg);
        }
    }

    fn top_bar(&mut self, f: &mut Frame, r: Rect) {
        let mut x = r.x + 2;
        x = f.buf.print(x, r.y, "◐", fg(theme::accent()), 1) + 2;
        let mut spans = vec![];
        for (i, t) in self.cfg.tabs.iter().enumerate() {
            let on = i == self.tab;
            let st = if on { fg(theme::text()).bold() } else if f.hover(Rect::new(x, r.y, len(&t.name), 1)) { fg(theme::muted()) } else { fg(theme::faint()) };
            let nx = f.buf.print(x, r.y, &t.name, st, 20);
            f.hit(Rect::new(x - 1, r.y, nx - x + 2, 1), ID_TAB + i as u32);
            spans.push((x, nx - x));
            x = nx + 3;
        }
        if let Some(&(sx, sw)) = spans.get(self.tab) {
            self.line_x.set(sx as f32);
            self.line_w.set(sw as f32);
        }
        // Hairline under the tabs; the active tab's underline glides to it.
        for cx in r.x..r.right() {
            f.buf.set(cx, r.y + 1, '─', fg(theme::line()));
        }
        let (lx, lw) = (self.line_x.get().round() as u16, self.line_w.get().round() as u16);
        for cx in lx..lx + lw {
            f.buf.set(cx, r.y + 1, '━', fg(theme::accent()));
        }
        let crumbs = match self.cur() {
            View::Desk => String::new(),
            View::Stock(i) => format!("› {}", self.sim.tickers[i].sym),
            View::Agent(99) => "› config".into(),
            View::Agent(i) => format!("› {} · {}", self.sim.agents.get(i).map_or("", |a| a.tool.as_str()), self.sim.agents.get(i).map_or("", |a| a.repo.as_str())),
            View::Player => format!("› {}", MODES[self.mode]),
            View::Income => "› Income".into(),
            View::Now => format!("› {}", self.now.now.as_ref().map_or("Now playing", |t| t.app.as_str())),
            View::Setup(i) => format!("› set up {}", setup::ITEMS[i].0),
            View::Connect => "› connect".into(),
            View::Gallery => "› add a card".into(),
        };
        f.buf.print(x, r.y, &crumbs, fg(theme::muted()), 40);
        let clock = clock_now();
        f.buf.print(r.right() - len(&clock) - 2, r.y, &clock, fg(theme::muted()), 8);
        let working = self.sim.agents.iter().filter(|a| a.state == cards::State::Working).count();
        let s = format!("{working} agent{} working", if working == 1 { "" } else { "s" });
        let sx = r.right() - len(&clock) - len(&s) - 6;
        f.buf.print(sx, r.y, &s, fg(theme::faint()), len(&s));
        f.buf.set(sx - 2, r.y, '●', fg(theme::accent().mix(theme::bg(), 0.4 - 0.4 * ((self.t * 2.0).sin() as f32 * 0.5 + 0.5))));
    }

    fn footer(&mut self, f: &mut Frame, r: Rect) {
        if let Some((msg, at)) = &self.note {
            if self.t - at < 4.0 {
                f.buf.print(r.x, r.y, msg, fg(theme::muted()), r.w);
                return;
            }
        }
        let keys: Vec<(&str, &str)> = if self.typing {
            vec![("ctrl+]", "leave terminal"), ("ctrl+space", "voice"), ("ctrl+k", "commands")]
        } else if self.edit {
            if self.picker.is_some() {
                vec![("↑↓", "pick"), ("enter", "add"), ("esc", "cancel")]
            } else {
                vec![("←→", "select"), ("h l", "move"), ("j k", "row"), ("+ -", "size"), ("[ ]", "height"), ("a", "add"), ("x", "remove"), ("n", "new row"), ("e", "done")]
            }
        } else {
            match self.cur() {
                View::Desk => vec![("tab", "focus"), ("enter", "open"), ("space", "play/pause"), ("[ ]", "tabs"), ("a", "add cards"), ("e", "edit layout"), ("t", "theme"), ("ctrl+k", "commands"), ("?", "help"), ("q", "quit")],
                View::Stock(_) => vec![("esc", "back"), ("←→", "range"), ("ctrl+k", "commands")],
                View::Player => vec![("esc", "back"), ("m", "mode"), ("space", "pause"), ("n", "next"), ("ctrl+k", "commands")],
                View::Agent(_) => vec![("ctrl+]", "back"), ("ctrl+space", "voice")],
                View::Now => vec![("esc", "back"), ("space", "play/pause"), ("n", "next"), ("p", "previous")],
                View::Income => vec![("esc", "back"), ("click", "open invoice")],
                View::Setup(_) | View::Connect => vec![("ctrl+]", "back")],
                View::Gallery => vec![("↑↓", "pick"), ("enter", "add to this tab"), ("esc", "back")],
            }
        };
        hints(f.buf, r.x + 1, r.y, r.w, &keys);
        let name = format!("{} theme", PRESETS[self.theme_to].name);
        f.buf.print(r.right() - len(&name) - 1, r.y, &name, fg(theme::faint()), len(&name));
    }

    fn desk(&mut self, f: &mut Frame, body: Rect) {
        let Some(tab) = self.cfg.tabs.get(self.tab).cloned() else { return };
        let sizes: Vec<Size> = tab.rows.iter().map(|r| match r.h { Height::Len(n) => Size::Len(n), Height::Fill(w) => Size::Fill(w) }).collect();
        let rows = body.rows(&sizes, 1);
        self.cells.clear();
        self.mod_hits.clear();
        for (ri, row) in tab.rows.iter().enumerate() {
            let ws: Vec<Size> = row.items.iter().map(|i| Size::Fill(i.weight)).collect();
            for (ci, r) in rows[ri].cols(&ws, 2).into_iter().enumerate() {
                self.cells.push((r, ri, ci));
            }
        }
        self.focus = self.focus.min(self.cells.len().saturating_sub(1));
        for idx in 0..self.cells.len() {
            let (r, ri, ci) = self.cells[idx];
            let it = tab.rows[ri].items[ci].clone();
            let focus = idx == self.focus;
            f.hit(r, ID_CARD + idx as u32);
            self.widget(f, r, &it, focus, idx);
            // On launch the cards rise out of the ground one after another.
            let k = since(self.t, 0.15 + idx as f64 * 0.06, 0.5);
            if k < 1.0 {
                cards::wash(f.buf, r, theme::bg(), 1.0 - k);
                for y in r.y..r.bottom() {
                    for x in r.x..r.right() {
                        if let Some(c) = f.buf.cell(x, y) {
                            c.fg = c.fg.mix(theme::bg(), 1.0 - k);
                        }
                    }
                }
            }
        }
        if let Some(p) = self.picker {
            if let Some(&(r, _, _)) = self.cells.get(self.focus) {
                let pr = Rect::new(r.x + 2, r.y + 2, 22, KINDS.len() as u16 + 2);
                f.buf.fill(pr, theme::raised());
                for (i, k) in KINDS.iter().enumerate() {
                    let st = if i == p { fg(theme::text()).bold() } else { fg(theme::muted()) };
                    f.buf.print(pr.x + 2, pr.y + 1 + i as u16, k, st, 18);
                }
            }
        }
    }

    fn widget(&mut self, f: &mut Frame, r: Rect, it: &Item, focus: bool, idx: usize) {
        let edit = self.edit;
        match it.kind.as_str() {
            "media" => {
                // What another app is playing; Enter or a click opens it large.
                let tint = self.now.cover.as_ref().map_or(theme::panel(), media::tint_of);
                card(f.buf, r, "", focus, edit);
                cards::wash(f.buf, r, tint, 0.16);
                let pos = self.now.pos();
                let (title, artist, l, playing) = match &self.now.now {
                    Some(t) => (t.title.clone(), format!("{} · {}", t.artist, t.app), Some(t.dur).filter(|d| *d > 0.0), t.playing),
                    None => ("Nothing playing".into(), "Spotify and Music show up here".into(), None, false),
                };
                let c = Rect::new(r.x + 2, r.y + 1, r.w.saturating_sub(4), r.h.saturating_sub(2).max(1));
                if self.now.now.is_some() {
                    cards::media_card(f, c, &title, &artist, pos, l, playing, (ID_OPEN, ID_PLAY, ID_SEEK));
                } else {
                    // Nothing to control, so no button or rail: just a quiet line.
                    let x = f.buf.print(c.x, c.y, &title, fg(theme::muted()), c.w) + 2;
                    f.buf.print(x, c.y, &artist, fg(theme::faint()), c.right().saturating_sub(x));
                }
                if focus {
                    f.buf.set(r.x, r.y + 1, '▎', fg(theme::accent()));
                }
            }
            "apps" => {
                card(f.buf, r, "", focus, edit);
                let c = Rect::new(r.x + 2, r.y + 1, r.w.saturating_sub(4), r.h.saturating_sub(2).max(1));
                cards::apps(f, c, &self.sim, ID_APP);
                if focus {
                    self.mark_sel(f, c, self.sel[2], 1, 1);
                }
            }
            "agents" => {
                let n = self.sim.agents.iter().filter(|a| a.state == cards::State::Working).count();
                let c = card(f.buf, r, &format!("Agents · {n} working"), focus, edit);
                self.sel_y.set(self.sel[0] as f32);
                let y = self.sel_y.get();
                cards::agents(f, c, &self.sim, focus.then_some(self.sel[0]), y, ID_AGENT);
            }
            "watch" => {
                let c = card(f.buf, r, "Watchlist", focus, edit);
                let layout = cards::grid(&it.args, self.sim.tickers.len());
                cards::watch(f, c, &self.sim, layout, ID_WATCH);
                if focus {
                    let t = cards::tile(c, layout, self.sel[1]);
                    f.buf.set(t.x.saturating_sub(2), t.y, '▎', fg(theme::accent()));
                }
            }
            "revenue" => {
                let title = match self.sim.errs.iter().find(|(s, _)| *s == "Stripe") {
                    Some((_, e)) => format!("Income · {e}"),
                    None if self.sim.income_live => "Income · Stripe".into(),
                    None if self.sim.live.contains(&"Stripe") => "Income · loading".into(),
                    None if self.demo => "Revenue".into(),
                    None => "Income · sample · enter to set up".into(),
                };
                let c = card(f.buf, r, &title, focus, edit);
                f.hit(c, ID_INCOME);
                cards::revenue(f, c, &self.sim);
            }
            "x" | "news" => {
                let title = match self.sim.errs.iter().find(|(s, _)| *s == "X") {
                    Some((_, e)) => format!("X · {e}"),
                    None if !self.sim.posts.is_empty() => "X · For you".into(),
                    None if self.demo => "X · Trending".into(),
                    None if self.sim.live.contains(&"X") => "X · loading".into(),
                    None => "X · not connected".into(),
                };
                let c = card(f.buf, r, &title, focus, edit);
                let live = self.demo || self.sim.live.contains(&"X");
                cards::news(f, c, &self.sim, live, ID_X);
            }
            "start" => {
                let rows: Vec<(String, String, bool)> = self.start_rows().into_iter().map(|(n, h, d, _)| (n, h, d)).collect();
                let done = rows.iter().filter(|r| r.2).count();
                let c = card(f.buf, r, &format!("Get started · {done}/{} done", rows.len()), focus, edit);
                cards::start(f, c, &rows, focus.then_some(self.sel[3]), ID_START);
            }
            "term" => {
                let cmd = if it.args.is_empty() { "shell".to_string() } else { it.args.join(" ") };
                let typing = focus && self.typing;
                let title = if typing { format!("{cmd}  · typing") } else { cmd.clone() };
                let c = card(f.buf, r, &title, focus, edit);
                #[cfg(unix)]
                {
                    let key = format!("{}:{cmd}", self.tab);
                    let inner = Rect::new(c.x - 1, c.y - 1, c.w + 2, c.h + 2);
                    let t = self.term(&key);
                    t.draw(f.buf, inner);
                    if t.exited {
                        f.buf.print(inner.x, inner.bottom() - 1, "exited · enter to restart", fg(theme::faint()), inner.w);
                    }
                }
                #[cfg(not(unix))]
                f.buf.print(c.x, c.y, "terminals need unix", fg(theme::faint()), c.w);
            }
            "player" => {
                let c = card(f.buf, r, "Player", focus, edit);
                let mode = it.args.first().and_then(|m| MODES.iter().position(|x| x == m)).unwrap_or(2);
                // `player split Talk` plays that source whenever this card is on screen.
                if let Some(want) = it.args.get(1) {
                    if let Some(i) = self.deck.items.iter().position(|m| m.label.eq_ignore_ascii_case(want)) {
                        if i != self.deck.active && self.trans.is_none() {
                            self.deck.open(i);
                        }
                    }
                }
                self.player_in(f, Rect::new(c.x - 1, c.y - 1, c.w + 2, c.h + 2), mode);
            }
            other if modules::exists(other) => {
                self.mods.get(other);
                let slot = &self.mods.slots[other];
                let title = slot.out.as_ref().and_then(|o| o.title.clone()).unwrap_or_else(|| other.into());
                let c = card(f.buf, r, &title, focus, edit);
                cards::module(f, c, slot, ID_MOD + idx as u32 * 32, &mut self.mod_hits);
            }
            other => {
                let c = card(f.buf, r, other, focus, edit);
                f.buf.print(c.x, c.y, &format!("unknown widget `{other}`. try: {}", KINDS.join(", ")), fg(theme::faint()), c.w);
            }
        }
    }

    fn mark_sel(&self, f: &mut Frame, c: Rect, i: usize, step: u16, cols: u16) {
        let (cx, cy) = (c.x + (i as u16 % cols) * (c.w / cols), c.y + (i as u16 / cols) * step);
        if cy < c.bottom() {
            f.buf.set(cx.saturating_sub(2), cy, '▎', fg(theme::accent()));
        }
    }

    fn back(&mut self, f: &mut Frame, r: Rect) {
        let b = Rect::new(r.x, r.y, 8, 1);
        let st = if f.hover(b) { fg(theme::text()) } else { fg(theme::muted()) };
        f.buf.print(r.x, r.y, "← Back", st, 8);
        f.hit(b, ID_BACK);
    }

    fn stock(&mut self, f: &mut Frame, body: Rect, i: usize) {
        f.buf.fill(body, theme::panel());
        let r = body.inset(3, 1);
        self.back(f, r);
        let Some(t) = self.sim.tickers.get(i) else { return };
        let inner = Rect::new(r.x, r.y + 2, r.w, r.h.saturating_sub(2));
        cards::stock(f, inner, t, self.range, self.morph.get(), &self.prev_series);
    }

    fn agent(&mut self, f: &mut Frame, body: Rect, i: usize) {
        self.back(f, body);
        let (title, detail) = match self.sim.agents.get(i) {
            Some(a) if i != 99 => (format!("{} · {}", a.tool, a.repo), a.task.clone()),
            _ => ("config".into(), config::path().display().to_string()),
        };
        f.buf.print(body.x + 10, body.y, &title, fg(theme::text()).bold(), 40);
        f.buf.print(body.x + 12 + len(&title), body.y, &detail, fg(theme::faint()), body.w / 2);
        #[cfg(unix)]
        {
            let key = format!("agent{i}:{}", if i == 99 { self.terms.iter().find(|(k, _)| k.starts_with("agent99:")).map_or(String::new(), |(k, _)| k[8..].to_string()) } else { self.agent_cmd(i) });
            let r = Rect::new(body.x, body.y + 2, body.w, body.h.saturating_sub(2));
            self.term(&key).draw(f.buf, r);
        }
    }

    fn setup_view(&mut self, f: &mut Frame, body: Rect, i: usize) {
        self.back(f, body);
        f.buf.print(body.x + 10, body.y, &format!("set up {}", setup::ITEMS[i].0), fg(theme::text()).bold(), 40);
        #[cfg(unix)]
        {
            let key = format!("setup{i}:{}", Self::setup_cmd(i));
            let r = Rect::new(body.x, body.y + 2, body.w, body.h.saturating_sub(2));
            self.term(&key).draw(f.buf, r);
        }
    }

    fn connect_view(&mut self, f: &mut Frame, body: Rect) {
        self.back(f, body);
        let name = self.conn.as_ref().map_or(String::new(), |(m, _)| modules::cap(m));
        f.buf.print(body.x + 10, body.y, &format!("connect {name}"), fg(theme::text()).bold(), 40);
        #[cfg(unix)]
        if let Some((_, c)) = self.conn.clone() {
            let r = Rect::new(body.x, body.y + 2, body.w, body.h.saturating_sub(2));
            self.term(&format!("connect:{c}")).draw(f.buf, r);
        }
    }

    /// Every module with its `# about:` line under its `# group:`; enter or a click adds it to this tab.
    fn gallery(&mut self, f: &mut Frame, body: Rect) {
        self.back(f, body);
        f.buf.print(body.x + 10, body.y, "Add a card", fg(theme::text()).bold(), 40);
        let on_desk = self.kinds().into_iter().map(String::from).collect::<Vec<_>>();
        // Display lines: a group heading (None) or an item index.
        let mut lines: Vec<(Option<usize>, &str)> = vec![];
        for (i, (_, _, g)) in self.gal_items.iter().enumerate() {
            if i == 0 || self.gal_items[i - 1].2 != *g {
                lines.push((None, g));
            }
            lines.push((Some(i), g));
        }
        let h = body.h.saturating_sub(3) as usize;
        let at = lines.iter().position(|l| l.0 == Some(self.gal)).unwrap_or(0);
        let top = (at + 1).saturating_sub(h);
        for (row, (i, g)) in lines.iter().skip(top).take(h).enumerate() {
            let y = body.y + 2 + row as u16;
            let Some(i) = *i else {
                f.buf.print(body.x + 2, y, g, fg(theme::faint()), body.w.saturating_sub(2));
                continue;
            };
            let (name, about, _) = &self.gal_items[i];
            let r = Rect::new(body.x, y, body.w, 1);
            let on = i == self.gal || f.hover(r);
            if on_desk.contains(name) {
                f.buf.print(body.x + 2, y, "✓", fg(theme::good()), 1);
            }
            f.buf.print(body.x + 4, y, name, fg(if on { theme::text() } else { theme::muted() }).bold(), 16);
            cards::fit(f.buf, body.x + 22, y, body.w.saturating_sub(22), about, fg(theme::faint()));
            if i == self.gal {
                f.buf.set(body.x, y, '▎', fg(theme::accent()));
            }
            f.hit(r, ID_GAL + i as u32);
        }
    }

    fn help_draw(&mut self, f: &mut Frame, a: Rect) {
        const HELP: [&str; 21] = [
            "Keys",
            "  tab focus a card · ↑↓ pick a row · enter open or set up · esc back",
            "  [ ] or 1-9 switch tabs · t theme · a add cards · e edit layout · q quit",
            "  space n p play, next, previous · m player mode",
            "  ctrl+k or : commands · ctrl+] leave a terminal · ? this help",
            "Cards",
            "  start your setup checklist · media, player what is playing",
            "  agents your Claude and Codex sessions · watch stock charts",
            "  revenue Stripe income · x trends and top posts · apps Discord, Gmail",
            "  term any command, live in the card",
            "Set up",
            "  A card that needs an account says  Connect <name>: focus it, press enter",
            "  Steps: stocks discord gmail stripe x path layout",
            "  Not connected? Focus the card and press enter.",
            "Modules",
            "  Press a (or ctrl+k, Add a card) to browse every module and add one",
            "  Or put a name in a row of the desk file, e.g.  row 8: weather, system",
            "  Your own: halftone module new <name> (any script that prints lines)",
            "Your desk file",
            "  halftone config opens it; saving reloads the desk live.",
            "esc or ? closes",
        ];
        let w = 74.min(a.w - 4);
        let h = (HELP.len() as u16 + 4).min(a.h - 2);
        let r = Rect::new(a.x + (a.w - w) / 2, a.y + (a.h - h) / 2, w, h);
        f.buf.fill(r, theme::raised());
        f.buf.print(r.x + 2, r.y + 1, "Help", fg(theme::text()).bold(), w - 4);
        for (i, l) in HELP.iter().enumerate().take(h as usize - 3) {
            let head = !l.starts_with(' ');
            let st = if head && i + 1 < HELP.len() { fg(theme::accent()).bold() } else if head { fg(theme::faint()) } else { fg(theme::muted()) };
            f.buf.print(r.x + 2, r.y + 3 + i as u16, l, st, w - 4);
        }
    }

    fn player(&mut self, f: &mut Frame, body: Rect) {
        self.player_in(f, body, self.mode);
    }

    fn player_in(&mut self, f: &mut Frame, r: Rect, mode: usize) {
        let tint = self.deck.item().map_or(theme::panel(), |i| i.tint);
        f.buf.fill(r, theme::bg().mix(tint, 0.14));
        let listen = matches!(MODES[mode], "lyrics" | "split");
        let whisper = self.cfg.mods.iter().any(|m| m == "lyrics");
        let lines = self.deck.lines(listen && whisper);
        let Some(it) = self.deck.item() else { return };
        let frame = it.bg_frame.as_ref().or(it.pb.as_ref().and_then(|p| p.frame.as_ref())).or(it.cover.as_ref()).cloned();
        let (title, artist, pos, l, playing) = match &it.pb {
            Some(pb) => (pb.src.title.clone(), pb.src.artist.clone().unwrap_or_default(), pb.pos, pb.len, pb.playing),
            None => (it.label.clone(), String::new(), 0.0, None, false),
        };
        let cover = it.cover.clone();
        let hud = Rect::new(r.x + 2, r.bottom().saturating_sub(4), r.w.saturating_sub(4), 3);
        match MODES[mode] {
            "video" => {
                if let Some(img) = &frame {
                    let used = crate::image::draw(f.buf, Rect::new(r.x, r.y, r.w, r.h), img, crate::image::Opts { mode: crate::image::Mode::Quad, ..Default::default() });
                    let _ = used;
                }
                // HUD fades away three seconds after the last touch.
                let k = ((self.t - self.hud_at - 3.0) / 0.6).clamp(0.0, 1.0) as f32;
                if k < 1.0 {
                    for y in hud.y.saturating_sub(1)..r.bottom() {
                        for x in r.x..r.right() {
                            if let Some(c) = f.buf.cell(x, y) {
                                c.bg = c.bg.mix(theme::bg(), 0.75 * (1.0 - k));
                                c.fg = c.fg.mix(theme::bg(), 0.5 * (1.0 - k));
                            }
                        }
                    }
                    self.hud(f, hud, &title, &artist, pos, l, playing, k);
                }
            }
            "lyrics" | "split" => {
                let split = MODES[mode] == "split";
                let lw = if split { r.w * 3 / 5 } else { r.w / 3 };
                let left = Rect::new(r.x, r.y, lw, r.h.saturating_sub(5));
                if split {
                    if let Some(img) = &frame {
                        crate::image::draw(f.buf, left, img, crate::image::Opts { mode: crate::image::Mode::Quad, ..Default::default() });
                    }
                } else {
                    let s = (left.w.min(left.h * 2)).saturating_sub(4) as u32;
                    let img = media::disc(cover.as_ref(), s.max(8), self.t as f32 * 0.6 * if playing { 1.0 } else { 0.0 }, theme::bg().mix(tint, 0.14));
                    crate::image::draw(f.buf, Rect::new(left.x + 2, left.y + 1, left.w - 2, left.h - 1), &img, crate::image::Opts { mode: crate::image::Mode::Quad, ..Default::default() });
                }
                let lr = Rect::new(r.x + lw + 4, r.y + 1, r.w.saturating_sub(lw + 6), r.h.saturating_sub(6));
                if lines.is_empty() {
                    let (head, sub) = if whisper { ("Listening for words…", "whisper is transcribing as it plays") } else { ("No lyrics for this one", "halftone lyrics <file> writes them, or turn on the lyrics mod") };
                    f.buf.print(lr.x, lr.y + lr.h / 3, head, fg(theme::muted()), lr.w);
                    f.buf.print(lr.x, lr.y + lr.h / 3 + 2, sub, fg(theme::faint()), lr.w);
                } else {
                    let dt = self.dt;
                    crate::deck::lyrics(f.buf, lr, &lines, pos, &mut self.scroll, dt);
                }
                self.hud(f, hud, &title, &artist, pos, l, playing, 0.0);
            }
            _ => {
                let s = (r.w.min(r.h * 2)).saturating_sub(12).min(72) as u32;
                let img = media::disc(cover.as_ref(), s.max(8), self.t as f32 * 0.6 * if playing { 1.0 } else { 0.0 }, theme::bg().mix(tint, 0.14));
                let dw = s as u16;
                let dr = Rect::new(r.x + (r.w.saturating_sub(dw)) / 2, r.y + 1, dw, (dw / 2).min(r.h.saturating_sub(6)));
                crate::image::draw(f.buf, dr, &img, crate::image::Opts { mode: crate::image::Mode::Quad, ..Default::default() });
                self.hud(f, hud, &title, &artist, pos, l, playing, 0.0);
            }
        }
    }

    fn now_view(&mut self, f: &mut Frame, r: Rect) {
        let Some(t) = self.now.now.clone() else {
            self.back(f, r);
            f.buf.print(r.x + 4, r.y + r.h / 3, "Nothing playing", fg(theme::text()).bold(), r.w);
            f.buf.print(r.x + 4, r.y + r.h / 3 + 2, "Play something in Spotify or Music and it shows up here.", fg(theme::faint()), r.w);
            return;
        };
        let tint = self.now.cover.as_ref().map_or(theme::bg(), media::tint_of);
        cards::wash(f.buf, r, tint, 0.12);
        self.back(f, r);
        // Cover on the left as a square (cells are twice as tall as wide), words on the right.
        let side = (r.h.saturating_sub(4)).min(r.w / 4);
        let cr = Rect::new(r.x + 4, r.y + 2, side * 2, side);
        match &self.now.cover {
            Some(img) => _ = crate::image::draw(f.buf, cr, img, crate::image::Opts { mode: crate::image::Mode::Quad, ..Default::default() }),
            None => {
                let img = media::disc(None, (side * 2) as u32, self.t as f32 * 0.6 * if t.playing { 1.0 } else { 0.0 }, theme::bg().mix(tint, 0.12));
                crate::image::draw(f.buf, cr, &img, crate::image::Opts { mode: crate::image::Mode::Quad, ..Default::default() });
            }
        }
        let tx = cr.right() + 6;
        let tw = r.right().saturating_sub(tx + 4);
        let mut y = r.y + 2 + side / 3;
        f.buf.print(tx, y, &t.app, fg(theme::faint()), tw);
        y += 2;
        crate::deck::marquee(f.buf, tx, y, tw, &t.title, f.time, fg(theme::text()).bold());
        y += 1;
        f.buf.print(tx, y, &t.artist, fg(theme::muted()), tw);
        y += 1;
        f.buf.print(tx, y, &t.album, fg(theme::faint()), tw);
        y += 3;
        let pos = self.now.pos();
        cards::media_card(f, Rect::new(tx, y, tw, 2), "", "", pos, Some(t.dur).filter(|d| *d > 0.0), t.playing, (0, ID_PLAY, ID_SEEK));
    }

    #[allow(clippy::too_many_arguments)]
    fn hud(&mut self, f: &mut Frame, r: Rect, title: &str, artist: &str, pos: f64, l: Option<f64>, playing: bool, fade: f32) {
        let m = |c: Color| c.mix(theme::bg(), fade);
        let tx = crate::deck::marquee(f.buf, r.x, r.y, r.w / 2, title, f.time, fg(m(theme::text())).bold());
        f.buf.print(tx + 2, r.y, artist, fg(m(theme::muted())), r.w / 2);
        cards::media_card(f, Rect::new(r.x, r.y + 1, r.w, 2), "", "", pos, l, playing, (0, ID_PLAY, ID_SEEK));
    }

    fn island_draw(&mut self, f: &mut Frame, a: Rect) {
        if !self.cfg.mods.iter().any(|m| m == "island") {
            return;
        }
        let (msg, tint, at) = match &self.island_msg {
            Some((m, c, at)) if self.t - at < 4.5 => (m.clone(), *c, *at),
            _ => {
                self.island.target = 0.0;
                if self.island.pos < 0.02 {
                    return;
                }
                (String::new(), theme::accent(), 0.0)
            }
        };
        if !msg.is_empty() {
            self.island.target = 1.0;
        }
        let k = self.island.pos.clamp(0.0, 1.2);
        let full = len(&msg) + 6;
        let w = (8.0 + (full as f32 - 8.0) * k).max(0.0) as u16;
        let x = a.x + (a.w.saturating_sub(w)) / 2;
        let r = Rect::new(x, a.y, w, 1);
        f.buf.fill(r, Color::rgb(0x050506).mix(theme::bg(), 0.2));
        if k > 0.6 && !msg.is_empty() {
            let reveal = ((k - 0.6) / 0.4).clamp(0.0, 1.0);
            let shown: String = msg.chars().take((msg.chars().count() as f32 * reveal) as usize).collect();
            f.buf.set(x + 2, a.y, '●', fg(tint));
            f.buf.print(x + 4, a.y, &shown, fg(theme::text()), w.saturating_sub(5));
        }
        let _ = at;
    }

    #[cfg(unix)]
    fn voice_draw(&mut self, f: &mut Frame, a: Rect) {
        let Some(v) = &self.voice else { return };
        let w = 34u16;
        let r = Rect::new(a.x + (a.w - w) / 2, a.bottom() - 3, w, 1);
        f.buf.fill(r, theme::raised());
        let pulse = 0.5 + 0.5 * (self.t * 4.0).sin() as f32;
        f.buf.set(r.x + 2, r.y, '●', fg(theme::bad().mix(theme::raised(), pulse * 0.5)));
        f.buf.print(r.x + 4, r.y, "Listening", fg(theme::text()), 10);
        // Live level as a braille wave, newest on the right.
        let lv = v.levels();
        let bars = ['⠀', '⣀', '⣤', '⣶', '⣿'];
        for (i, l) in lv.iter().rev().take(16).enumerate() {
            let c = bars[((l * 4.0).round() as usize).min(4)];
            f.buf.set(r.right() - 3 - i as u16, r.y, c, fg(theme::muted()));
        }
    }

    fn palette_draw(&mut self, f: &mut Frame, a: Rect) {
        let list = self.filtered();
        let Some((input, sel)) = &mut self.palette else { return };
        *sel = (*sel).min(list.len().saturating_sub(1));
        let w = 56.min(a.w - 4);
        let h = (list.len().min(10) as u16) + 4;
        let r = Rect::new(a.x + (a.w - w) / 2, a.y + a.h / 5, w, h);
        f.buf.fill(r, theme::raised());
        let q = if input.value.is_empty() { "Type a command…".to_string() } else { input.value.clone() };
        let st = if input.value.is_empty() { fg(theme::faint()) } else { fg(theme::text()) };
        f.buf.print(r.x + 2, r.y + 1, "›", fg(theme::accent()), 1);
        f.buf.print(r.x + 4, r.y + 1, &q, st, w - 6);
        for x in r.x + 1..r.right() - 1 {
            f.buf.set(x, r.y + 2, '─', fg(theme::line()));
        }
        let start = sel.saturating_sub(9);
        for (row, (i, (name, _))) in list.iter().enumerate().skip(start).take(10).enumerate() {
            let y = r.y + 3 + row as u16;
            let on = i == *sel;
            if on {
                f.buf.fill(Rect::new(r.x + 1, y, w - 2, 1), theme::line());
            }
            f.buf.print(r.x + 3, y, name, if on { fg(theme::text()).bold() } else { fg(theme::muted()) }, w - 6);
            f.hit(Rect::new(r.x + 1, y, w - 2, 1), ID_PAL + i as u32);
        }
        let _ = Style::new();
    }
}

#[cfg(unix)]
extern "C" {
    fn localtime_r(t: *const i64, tm: *mut u8) -> *mut u8;
}

fn clock_now() -> String {
    if std::env::var("HALFTONE_DEMO").is_ok() {
        return "9:41".into();
    }
    #[cfg(unix)]
    {
        let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let mut tm = [0u8; 64];
        // SAFETY: tm is larger than any libc's struct tm; localtime_r only writes into it.
        unsafe { localtime_r(&t, tm.as_mut_ptr()) };
        let f = |i: usize| i32::from_ne_bytes(tm[i * 4..i * 4 + 4].try_into().unwrap());
        format!("{}:{:02}", f(2), f(1))
    }
    #[cfg(not(unix))]
    String::new()
}
