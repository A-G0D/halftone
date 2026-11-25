//! A desk of widgets in one terminal: a media card with tabs and a row of app shortcuts on top,
//! then coding agents, a watchlist, revenue and news. Every panel is a few calls into `deck` and `widgets`.
//! `cargo run --release --example desk [-- <music> <video> <talk>]` (defaults in ~/.cache/halftone/media)
//! tab next media · space pause · n/p ±10 s · 1-6 open an app · t theme · q quit. Needs ffmpeg.
//! HALFTONE_MUTE=1 for silence. Agents, prices, revenue and news are sample data on a script.

use halftone::deck;
use halftone::media::{self, Playback, Video};
use halftone::prelude::*;
use halftone::theme::PRESETS;
use std::sync::mpsc::{self, Receiver};
use std::{thread, time::Duration};

const IDS: Transport = Transport { prev: 1, play: 2, next: 3 };
const SEEK: u32 = 4;
const VOL: u32 = 5;
const TABS: u32 = 10;
const APPS_ID: u32 = 20;
const SPIN: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Shortcut tiles: name, letter, tile colour, letter colour, link.
const APPS: [(&str, char, u32, u32, &str); 6] = [
    ("Discord", 'D', 0x5865f2, 0xffffff, "https://discord.com/app"),
    ("Instagram", 'I', 0xe1306c, 0xffffff, "https://instagram.com"),
    ("X", 'X', 0xe7e9ea, 0x0b0c0e, "https://x.com"),
    ("YouTube", 'Y', 0xff0033, 0xffffff, "https://youtube.com"),
    ("Reddit", 'R', 0xff4500, 0xffffff, "https://reddit.com"),
    ("Twitch", 'T', 0x9146ff, 0xffffff, "https://twitch.tv"),
];

#[derive(Clone, Copy, PartialEq)]
enum State {
    Working,
    Waiting,
    Done,
}

struct Agent {
    tool: &'static str,
    repo: &'static str,
    task: String,
    state: State,
    /// Seconds of work shown in the elapsed column.
    secs: f64,
    ctx: f32,
    cost: f32,
}

struct Ticker {
    sym: &'static str,
    name: &'static str,
    open: f32,
    hist: Vec<f32>,
    /// Smoothed drift, so the line trends instead of jittering.
    vel: f32,
}

struct News {
    topic: &'static str,
    head: &'static str,
    posts: f32,
    /// Time it broke, for the fade of its accent bar; negative for older stories.
    at: f64,
}

/// One media tab: its file, its player once opened, and its cover.
struct Media {
    label: &'static str,
    src: String,
    pb: Option<Playback>,
    cover: Option<Image>,
    video: bool,
}

struct Desk {
    media: Vec<Media>,
    tab: usize,
    covers: Receiver<(usize, Option<Image>)>,
    spec: Spectrum,
    unread: [u32; 6],
    agents: Vec<Agent>,
    tickers: Vec<Ticker>,
    rev: Vec<f32>,
    today: f32,
    sale: Option<(f64, &'static str)>,
    news: Vec<News>,
    rng: u64,
    clock: f64,
    next_tick: f64,
    step: usize,
    theme: usize,
}

impl Desk {
    fn new(srcs: [String; 3]) -> Desk {
        let (tx, covers) = mpsc::channel();
        let media: Vec<Media> = srcs.into_iter().zip(["Music", "Video", "Talks"]).enumerate().map(|(i, (src, label))| {
            let (tx, s) = (tx.clone(), src.clone());
            thread::spawn(move || _ = tx.send((i, grab(&s, 0.0))));
            Media { label, src, pb: None, cover: None, video: label == "Video" }
        }).collect();
        let mut d = Desk {
            media, tab: 0, covers, spec: Spectrum::default(), unread: [3, 0, 12, 0, 1, 0],
            agents: vec![
                Agent { tool: "claude", repo: "halftone", task: "reading src/deck.rs".into(), state: State::Working, secs: 724.0, ctx: 0.41, cost: 1.12 },
                Agent { tool: "codex", repo: "billing-api", task: "running cargo test  18/32".into(), state: State::Working, secs: 251.0, ctx: 0.22, cost: 0.38 },
                Agent { tool: "claude", repo: "site", task: "done · 3 files changed".into(), state: State::Done, secs: 1310.0, ctx: 0.67, cost: 2.04 },
                Agent { tool: "codex", repo: "infra", task: "planning terraform changes".into(), state: State::Working, secs: 96.0, ctx: 0.12, cost: 0.17 },
                Agent { tool: "claude", repo: "mobile", task: "fixing flaky login test".into(), state: State::Working, secs: 1888.0, ctx: 0.58, cost: 3.40 },
                Agent { tool: "codex", repo: "docs", task: "writing migration guide".into(), state: State::Working, secs: 402.0, ctx: 0.31, cost: 0.66 },
                Agent { tool: "claude", repo: "analytics", task: "done · dashboard query 4x faster".into(), state: State::Done, secs: 2710.0, ctx: 0.74, cost: 4.12 },
            ],
            tickers: Vec::new(), rev: Vec::new(), today: 1284.0, sale: None,
            news: vec![
                News { topic: "Science", head: "Eclipse photos are taking over timelines", posts: 48_200.0, at: -99.0 },
                News { topic: "Technology", head: "A new Rust release is out today", posts: 21_700.0, at: -99.0 },
                News { topic: "Business", head: "Markets open higher on chip demand", posts: 96_400.0, at: -99.0 },
                News { topic: "Weather", head: "Heat wave reaches the West Coast", posts: 33_100.0, at: -99.0 },
            ],
            rng: 0x9e37_79b9_7f4a_7c15, clock: 0.0, next_tick: 0.0, step: 0, theme: 0,
        };
        d.open(0);
        for (sym, name, p) in [("NVDA", "Nvidia", 138.2), ("AAPL", "Apple", 229.4), ("TSLA", "Tesla", 251.9), ("BTC", "Bitcoin", 68420.0)] {
            let mut t = Ticker { sym, name, open: p, hist: vec![p], vel: 0.0 };
            for _ in 0..89 {
                d.walk(&mut t);
            }
            t.open = t.hist[30];
            d.tickers.push(t);
        }
        let mut v = 38_000.0;
        for _ in 0..30 {
            v *= 1.0 + 0.004 + d.noise() * 0.012;
            d.rev.push(v);
        }
        d
    }

    /// xorshift in -1..1, so the sample data replays the same every run.
    fn noise(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 20_001) as f32 / 10_000.0 - 1.0
    }

    fn walk(&mut self, t: &mut Ticker) {
        t.vel = t.vel * 0.8 + self.noise() * 0.2;
        let last = *t.hist.last().unwrap();
        t.hist.push(last * (1.0 + t.vel * 0.006 + 0.0003));
    }

    /// Switches to tab `i`, pausing the one before and opening `i` on first use.
    fn open(&mut self, i: usize) {
        if let Some(pb) = self.media[self.tab].pb.as_mut().filter(|p| p.playing) {
            let _ = pb.toggle();
        }
        self.tab = i;
        let m = &mut self.media[i];
        match &mut m.pb {
            Some(pb) if !pb.playing => _ = pb.toggle(),
            Some(_) => {}
            None => m.pb = Playback::open(&m.src, m.video.then_some((48, 27)), true).ok(),
        }
    }

    /// One beat of the script: prices move, agents make progress, news breaks or a sale lands.
    fn beat(&mut self) {
        self.step += 1;
        let mut ts = std::mem::take(&mut self.tickers);
        for t in &mut ts {
            self.walk(t);
            t.hist.remove(0);
        }
        self.tickers = ts;
        let s = self.step;
        let now = self.clock;
        for (i, n) in self.news.iter_mut().enumerate() {
            n.posts *= 1.0 + 0.002 * (4 - i.min(3)) as f32;
        }
        let a = &mut self.agents;
        match s {
            4 => a[0].task = "editing src/deck.rs".into(),
            7 => a[4].task = "running the login suite 40x".into(),
            6 => a[3] = Agent { task: "needs approval · terraform apply".into(), state: State::Waiting, ..std::mem::replace(&mut a[3], dummy()) },
            9 => self.unread[0] += 1,
            11 => a[0].task = "running cargo test".into(),
            12 => self.news.insert(0, News { topic: "Breaking", head: "Major cloud outage, services recovering", posts: 8_900.0, at: now }),
            19 => a[0].task = "done · 32 tests pass".into(),
            _ => {}
        }
        if s == 19 {
            a[0].state = State::Done;
        }
        if a[1].state == State::Working {
            let n = (18 + s * 2 / 3).min(32);
            a[1].task = if n == 32 { "done · 32/32 tests pass".into() } else { format!("running cargo test  {n}/32") };
            if n == 32 {
                a[1].state = State::Done;
            }
        }
        if s % 5 == 2 {
            let plan = ["Pro plan", "Team plan", "Pro plan", "Lifetime"][(s / 5) % 4];
            let amt = match plan { "Team plan" => 99.0, "Lifetime" => 249.0, _ => 29.0 };
            self.today += amt;
            *self.rev.last_mut().unwrap() += amt;
            self.sale = Some((now, plan));
        }
        if s % 3 == 0 {
            self.unread[2] += 1;
        }
        self.news.truncate(6);
    }

    fn pb(&mut self) -> Option<&mut Playback> {
        self.media[self.tab].pb.as_mut()
    }
}

fn dummy() -> Agent {
    Agent { tool: "", repo: "", task: String::new(), state: State::Done, secs: 0.0, ctx: 0.0, cost: 0.0 }
}

/// The first frame ffmpeg gives from `at` (embedded album art included), waiting up to 4 s.
fn grab(src: &str, at: f64) -> Option<Image> {
    let mut v = Video::open(src, 64, 64, at).ok()?;
    for _ in 0..80 {
        if let Some(i) = v.frame() {
            return Some(i);
        }
        if v.ended() {
            return v.frame();
        }
        thread::sleep(Duration::from_millis(50));
    }
    None
}

/// 48210.4 -> "48,210"
fn money(v: f32) -> String {
    let s = (v.round() as i64).to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// 48210 -> "48.2K"
fn short(v: f32) -> String {
    if v >= 1000.0 { format!("{:.1}K", v / 1000.0) } else { format!("{v:.0}") }
}

fn clock(secs: f64) -> String {
    let m = (secs / 60.0) as u32;
    if m >= 60 { format!("{}h{:02}m", m / 60, m % 60) } else { format!("{m}m") }
}

fn len(s: &str) -> u16 {
    s.chars().count() as u16
}

fn launch(url: &str) {
    let cmd = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = std::process::Command::new(cmd).arg(url).spawn();
}

impl App for Desk {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match *ev {
            Event::Tick(dt) => {
                self.clock += dt;
                for a in &mut self.agents {
                    if a.state == State::Working {
                        a.secs += dt;
                        a.ctx = (a.ctx + dt as f32 * 0.004).min(0.95);
                        a.cost += dt as f32 * 0.002;
                    }
                }
                if self.clock >= self.next_tick {
                    self.next_tick += 1.0;
                    self.beat();
                }
                while let Ok((i, c)) = self.covers.try_recv() {
                    self.media[i].cover = c;
                }
                for m in &mut self.media {
                    if let Some(pb) = &mut m.pb {
                        pb.tick(dt);
                    }
                }
                if let Some(pb) = self.media[self.tab].pb.as_mut() {
                    self.spec.update(&media::spectrum(&pb.samples(2048), 16), dt as f32);
                }
            }
            Event::Click { id: SEEK, x, rect, .. } | Event::Drag { id: SEEK, x, rect, .. } => {
                if let Some(pb) = self.pb() {
                    let _ = pb.seek_to(rail_time(x, rect, pb.len.unwrap_or(0.0)));
                }
            }
            Event::Click { id: VOL, x, rect, .. } => {
                if let Some(pb) = self.pb() {
                    let _ = pb.set_volume((deck::volume_value(x, rect) * 100.0) as i32);
                }
            }
            Event::Click { id, .. } if id == IDS.play => _ = self.pb().map(Playback::toggle),
            Event::Click { id, .. } if (TABS..TABS + 3).contains(&id) => self.open((id - TABS) as usize),
            Event::Click { id, .. } if (APPS_ID..APPS_ID + 6).contains(&id) => launch(APPS[(id - APPS_ID) as usize].4),
            _ if ev.key() == Some(KeyCode::Tab) => self.open((self.tab + 1) % self.media.len()),
            _ => match ev.ch() {
                Some(' ') => _ = self.pb().map(Playback::toggle),
                Some('n') => _ = self.pb().map(|p| p.seek(10.0)),
                Some('p') => _ = self.pb().map(|p| p.seek(-10.0)),
                Some(c @ '1'..='6') => launch(APPS[c as usize - '1' as usize].4),
                Some('t') => {
                    self.theme = (self.theme + 1) % PRESETS.len();
                    theme::set(PRESETS[self.theme]);
                }
                Some('q') => ctx.quit(),
                _ => {}
            },
        }
    }

    fn view(&mut self, f: &mut Frame) {
        let a = f.area;
        f.buf.fill(a, theme::bg());
        if a.w < 80 || a.h < 24 {
            f.buf.print(a.x + 1, a.y, "make the terminal bigger", fg(theme::muted()), a.w);
            return;
        }
        let page = a.inset(1, 0);
        let rows = page.rows(&[Len(8), Fill(1), Fill(1)], 0);
        let top = rows[0].cols(&[Fill(2), Fill(1)], 1);
        self.playing(f, top[0]);
        self.apps(f, top[1]);
        let mid = rows[1].cols(&[Fill(2), Fill(1)], 1);
        self.agents_panel(f, mid[0]);
        self.stocks(f, mid[1]);
        let low = rows[2].cols(&[Fill(2), Fill(1)], 1);
        self.revenue(f, low[0]);
        self.news(f, low[1]);
    }
}

impl Desk {
    fn playing(&mut self, f: &mut Frame, r: Rect) {
        let c = panel(f.buf, r, "", false);
        let m = &self.media[self.tab];
        // Cover art 12×6 (12×12 pixels); a video tab shows its live frame instead.
        let art = if m.video { m.pb.as_ref().and_then(|p| p.frame.as_ref()).or(m.cover.as_ref()) } else { m.cover.as_ref() };
        let icon = deck::cover(f.buf, Rect::new(c.x, c.y, 12, 6), art, f.time);
        let x = icon.right().max(c.x + 12) + 2;
        let right = c.right();
        let labels: Vec<&str> = self.media.iter().map(|m| m.label).collect();
        tabs(f, x - 1, c.y, &labels, self.tab, TABS);
        let sw = 20u16;
        self.spec.draw(f.buf, Rect::new(right - sw, c.y, sw, 1));
        let Some(pb) = &self.media[self.tab].pb else {
            f.buf.print(x, c.y + 2, "nothing playing", fg(theme::muted()), c.w);
            return;
        };
        // Title and artist in big type on one line; when both don't fit they roll together.
        let (title, artist) = (&pb.src.title, pb.src.artist.as_deref().unwrap_or(""));
        let w = right.saturating_sub(x);
        if big_width(title) + 3 + big_width(artist) <= w {
            let ax = big(f.buf, x, c.y + 2, w, title, theme::text(), f.time) + 3;
            big(f.buf, ax, c.y + 2, right - ax, artist, theme::muted(), f.time);
        } else {
            big(f.buf, x, c.y + 2, w, &format!("{title} · {artist}"), theme::text(), f.time);
        }
        let y = c.y + 5;
        let tx = deck::transport(f, x, y, pb.playing, IDS) + 2;
        let (now, total) = (media::clock(pb.pos), pb.len.map_or("live".into(), media::clock));
        let nx = f.buf.print(tx, y, &now, fg(theme::muted()), 8) + 2;
        let tlen = len(&total);
        let rail_end = (right - 7).saturating_sub(tlen + 2);
        deck::rail(f, Rect::new(nx, y, rail_end.saturating_sub(nx), 1), pb.progress(), pb.len, SEEK);
        f.buf.print(rail_end + 1, y, &total, fg(theme::muted()), tlen);
        deck::volume(f, Rect::new(right - 5, y, 5, 1), pb.volume as f32 / 100.0, VOL);
    }

    /// Top right: one tile per app, two columns, with unread counts. 1-6 or a click opens it.
    fn apps(&self, f: &mut Frame, r: Rect) {
        let c = panel(f.buf, r, "Open · 1-6", false);
        let cw = (c.w + 2) / 2;
        for (i, &(name, letter, tile, ink, _)) in APPS.iter().enumerate() {
            let (x, y) = (c.x + (i as u16 % 2) * cw, c.y + (i as u16 / 2) * 2);
            let cell = Rect::new(x, y, cw.saturating_sub(2), 1);
            if f.hover(cell) {
                f.buf.fill(cell, theme::raised());
            }
            let t = Color::rgb(tile);
            f.buf.print(x, y, &format!(" {letter} "), Style::new().fg(Color::rgb(ink)).bg(t).bold(), 3);
            f.buf.print(x + 4, y, name, fg(theme::text()).bold(), cw);
            let n = self.unread[i];
            if n > 0 {
                let badge = format!(" {n} ");
                let bx = cell.right() - len(&badge);
                f.buf.print(bx, y, &badge, Style::new().fg(theme::bg()).bg(t.mix(theme::text(), 0.15)).bold(), len(&badge));
            }
            f.hit(cell, APPS_ID + i as u32);
        }
    }

    fn agents_panel(&self, f: &mut Frame, r: Rect) {
        let working = self.agents.iter().filter(|a| a.state == State::Working).count();
        let c = panel(f.buf, r, &format!("Coding agents · {working} working"), false);
        let spin = SPIN[(f.time * 12.0) as usize % SPIN.len()];
        let mut y = c.y + 1;
        for a in &self.agents {
            if y >= c.bottom() {
                break;
            }
            let (glyph, col) = match a.state {
                State::Working => (spin, theme::accent()),
                State::Waiting => ('●', theme::warn()),
                State::Done => ('✓', theme::good()),
            };
            f.buf.set(c.x, y, glyph, fg(col).bold());
            let x = f.buf.print(c.x + 2, y, a.tool, fg(theme::text()).bold(), 8);
            let x = f.buf.print(x.max(c.x + 10), y, a.repo, fg(theme::muted()), 14);
            let meta = format!("{:>3.0}%  {:>6}  ${:.2}", a.ctx * 100.0, clock(a.secs), a.cost);
            let mx = c.right() - len(&meta);
            f.buf.print(mx, y, &meta, fg(theme::muted()), len(&meta));
            // Context window used, as a thin bar left of its percentage.
            let gx = mx.saturating_sub(12);
            gauge(f.buf, Rect::new(gx, y, 10, 1), a.ctx, theme::faint(), theme::muted());
            let task_x = x.max(c.x + 25);
            let task_col = match a.state { State::Waiting => theme::warn(), State::Done => theme::muted(), _ => theme::text() };
            deck::marquee(f.buf, task_x, y, gx.saturating_sub(task_x + 2), &a.task, f.time, fg(task_col));
            y += 2;
        }
    }

    /// Four tiles in a 2×2 grid: symbol and change, the price in big type, then the day's line.
    fn stocks(&self, f: &mut Frame, r: Rect) {
        let c = panel(f.buf, r, "Watchlist", false);
        let grid = c.rows(&[Fill(1), Fill(1)], 1);
        for (i, t) in self.tickers.iter().enumerate() {
            let tile = grid[i / 2].cols(&[Fill(1), Fill(1)], 3)[i % 2];
            let last = *t.hist.last().unwrap();
            let ch = (last / t.open - 1.0) * 100.0;
            let col = if ch >= 0.0 { theme::good() } else { theme::bad() };
            let x = f.buf.print(tile.x, tile.y, t.sym, fg(theme::text()).bold(), 6);
            f.buf.print(x + 1, tile.y, t.name, fg(theme::faint()), tile.w / 2);
            let pct = format!("{}{ch:.2}%", if ch >= 0.0 { "▲ " } else { "▼ " });
            f.buf.print(tile.right() - len(&pct), tile.y, &pct, fg(col), len(&pct));
            let price = if last > 10_000.0 { money(last) } else { format!("{last:.2}") };
            big(f.buf, tile.x, tile.y + 2, tile.w, &price, theme::text(), 0.0);
            let lo = t.hist.iter().cloned().fold(f32::MAX, f32::min);
            let norm: Vec<f32> = (0..t.hist.len()).map(|i| t.hist[i.saturating_sub(5)..=i].iter().sum::<f32>() / (i.min(5) + 1) as f32 - lo).collect();
            let max = norm.iter().cloned().fold(0.0, f32::max) * 1.05;
            let cr = Rect::new(tile.x, tile.y + 6, tile.w, tile.h.saturating_sub(6));
            chart(f.buf, cr, &norm, max, col, col.mix(theme::panel(), 0.5));
        }
    }

    fn revenue(&self, f: &mut Frame, r: Rect) {
        let c = panel(f.buf, r, "Revenue · 30 days", false);
        let mrr = *self.rev.last().unwrap();
        let growth = (mrr / self.rev[0] - 1.0) * 100.0;
        let x = stat(f.buf, c.x, c.y, "MRR", &format!("${}", money(mrr)), theme::text());
        let x = stat(f.buf, x + 2, c.y, "30-day growth", &format!("+{growth:.1}%"), theme::good());
        let x = stat(f.buf, x + 2, c.y, "Today", &format!("${}", money(self.today)), theme::text());
        if let Some((at, plan)) = self.sale {
            let k = ((f.time - at) / 2.5).clamp(0.0, 1.0) as f32;
            if k < 1.0 {
                let s = format!("+ new sale · {plan}");
                f.buf.print(x + 2, c.y + 1, &s, fg(theme::accent().mix(theme::panel(), k)), len(&s));
            }
        }
        let streams = [("Stripe", 0.64), ("App Store", 0.2), ("Sponsors", 0.09), ("Ads", 0.07)];
        let lw = 30.min(c.w / 3);
        let chart_r = Rect::new(c.x, c.y + 3, c.w.saturating_sub(lw + 3), c.h.saturating_sub(3));
        let lo = self.rev.iter().cloned().fold(f32::MAX, f32::min) * 0.97;
        let norm: Vec<f32> = self.rev.iter().map(|v| v - lo).collect();
        let max = norm.iter().cloned().fold(0.0, f32::max) * 1.05;
        chart(f.buf, chart_r, &norm, max, theme::accent(), theme::accent());
        let sx = c.right() - lw;
        let mut y = c.y + 3;
        for (name, share) in streams {
            if y + 1 >= c.bottom() {
                break;
            }
            let v = format!("${}", money(mrr * share));
            f.buf.print(sx, y, name, fg(theme::text()), lw);
            f.buf.print(c.right() - len(&v), y, &v, fg(theme::muted()), len(&v));
            gauge(f.buf, Rect::new(sx, y + 1, lw, 1), share / 0.64, theme::blue().mix(theme::panel(), 0.4), theme::blue());
            y += 3;
        }
    }

    /// What's big on X: topic, headline, post count; a breaking story lands with an accent bar.
    fn news(&self, f: &mut Frame, r: Rect) {
        let c = panel(f.buf, r, "News on X · trending", false);
        let mut y = c.y + 1;
        for n in &self.news {
            if y + 2 >= c.bottom() {
                break;
            }
            let fresh = (1.0 - (f.time - n.at) / 5.0).clamp(0.0, 1.0) as f32;
            let breaking = n.at >= 0.0;
            let bar = theme::panel().mix(theme::bad(), if breaking { fresh.max(0.6) } else { 0.0 });
            for k in 0..3 {
                f.buf.set(c.x, y + k, '▍', fg(bar));
            }
            let tc = if breaking { theme::bad() } else { theme::muted() };
            f.buf.print(c.x + 2, y, &format!("{} · trending", n.topic), fg(tc), c.w - 2);
            deck::marquee(f.buf, c.x + 2, y + 1, c.w - 2, n.head, f.time, fg(theme::text()).bold());
            f.buf.print(c.x + 2, y + 2, &format!("{} posts", short(n.posts)), fg(theme::faint()), c.w - 2);
            y += 4;
        }
    }
}

fn source() -> [String; 3] {
    let dir = format!("{}/.cache/halftone/media", std::env::var("HOME").unwrap_or_default());
    let mut args = std::env::args().skip(1);
    ["nightowl.m4a", "artemis.mp4", "moon.m4a"].map(|f| args.next().unwrap_or_else(|| format!("{dir}/{f}")))
}

halftone::main!(Desk::new(source()));
