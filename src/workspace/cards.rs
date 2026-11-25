//! The desk's cards and their data. Cards are flat surfaces on the page ground: no boxes, a quiet
//! title, one accent, thin lines instead of blocks. Every number eases; new rows slide in.

use crate::motion::{since, Tween};
use crate::widgets::chart;
use crate::{fg, theme, Buffer, Color, Frame, Rect, Style};

pub const SPIN: [&str; 4] = ["●∙∙", "∙●∙", "∙∙●", "∙●∙"];

/// Brand colours pulled toward the theme so a row of apps reads as one palette, not a sticker sheet.
pub fn tone(hex: u32) -> Color {
    Color::rgb(hex).mix(theme::muted(), 0.45).mix(theme::panel(), 0.15)
}

/// A flat card: panel surface, quiet title, focus shown by the title alone. Returns the inside.
pub fn card(b: &mut Buffer, r: Rect, title: &str, focus: bool, edit: bool) -> Rect {
    b.fill(r, theme::panel());
    if edit {
        let line = if focus { theme::accent() } else { theme::line() };
        crate::widgets::panel(b, r, "", false);
        // Re-tint the border the panel drew so the selected card reads as picked up.
        for x in r.x..r.right() {
            for y in [r.y, r.bottom().saturating_sub(1)] {
                if let Some(c) = b.cell(x, y) {
                    c.fg = line;
                }
            }
        }
        for y in r.y..r.bottom() {
            for x in [r.x, r.right().saturating_sub(1)] {
                if let Some(c) = b.cell(x, y) {
                    c.fg = line;
                }
            }
        }
    }
    let st = if focus { fg(theme::text()).bold() } else { fg(theme::muted()) };
    b.print(r.x + 2, r.y + 1, title, st, r.w.saturating_sub(4));
    Rect::new(r.x + 2, r.y + 3, r.w.saturating_sub(4), r.h.saturating_sub(4))
}

/// Thin meter: a heavy line for the filled part, a hairline for the rest.
pub fn meter(b: &mut Buffer, x: u16, y: u16, w: u16, v: f32, col: Color) {
    let fill = (v.clamp(0.0, 1.0) * w as f32).round() as u16;
    for i in 0..w {
        let (ch, c) = if i < fill { ('━', col) } else { ('─', theme::line()) };
        b.set(x + i, y, ch, fg(c));
    }
}

/// Key hints: the key in body text, what it does in faint. Returns the x after them.
pub fn hints(b: &mut Buffer, x: u16, y: u16, max: u16, keys: &[(&str, &str)]) -> u16 {
    let end = x + max;
    let mut cx = x;
    for (k, label) in keys {
        let need = (k.chars().count() + label.chars().count() + 4) as u16;
        if cx + need > end {
            break;
        }
        cx = b.print(cx, y, k, fg(theme::text()), max);
        cx = b.print(cx + 1, y, label, fg(theme::faint()), max) + 3;
    }
    cx
}

pub fn len(s: &str) -> u16 {
    s.chars().count() as u16
}

pub fn money(v: f32) -> String {
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

pub fn short(v: f32) -> String {
    if v >= 1000.0 { format!("{:.1}K", v / 1000.0) } else { format!("{v:.0}") }
}

pub fn clock(secs: f64) -> String {
    let m = (secs / 60.0) as u32;
    if m >= 48 * 60 { format!("{}d", m / 1440) } else if m >= 60 { format!("{}h {:02}m", m / 60, m % 60) } else { format!("{m}m") }
}

#[derive(Clone, Copy, PartialEq)]
pub enum State {
    Working,
    Waiting,
    Done,
}

pub struct Agent {
    pub tool: String,
    pub repo: String,
    /// What reopens a real session in its own folder; empty for the sample rows.
    pub cmd: String,
    pub task: String,
    /// What a real session is doing now, shown after its title.
    pub doing: String,
    pub state: State,
    pub secs: f64,
    pub ctx: f32,
    /// When the row last changed, for its highlight.
    pub changed: f64,
}

pub struct Ticker {
    pub sym: String,
    pub name: String,
    /// The day's baseline: yesterday's close.
    pub open: f32,
    pub hist: Vec<f32>,
    vel: f32,
    pub price: Tween,
    /// Live quotes per range (1D 5D 1M 6M 1Y) as they arrive; empty on sample tickers.
    pub live: Vec<Option<super::quotes::Quote>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Invoice {
    /// Unix seconds.
    pub ts: f64,
    pub who: String,
    pub what: String,
    pub amount: f32,
    pub status: String,
    /// Stripe's hosted page for it; empty on samples.
    pub url: String,
}

pub fn now_unix() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// "5m ago", "3h ago", "yesterday", "12d ago".
pub fn ago(ts: f64) -> String {
    let m = ((now_unix() - ts) / 60.0).max(0.0) as u64;
    match m {
        0 => "just now".into(),
        1..=59 => format!("{m}m ago"),
        60..=1439 => format!("{}h ago", m / 60),
        1440..=2879 => "yesterday".into(),
        _ => format!("{}d ago", m / 1440),
    }
}

pub struct News {
    pub topic: &'static str,
    pub head: &'static str,
    pub posts: f32,
    pub at: f64,
}

pub struct App {
    pub name: &'static str,
    pub tint: u32,
    pub url: &'static str,
    pub preview: String,
    pub unread: Tween,
    pub at: f64,
}

/// Sample data on a fixed script, so a recording replays the same every time.
pub struct Sim {
    pub agents: Vec<Agent>,
    pub tickers: Vec<Ticker>,
    pub rev: Vec<f32>,
    pub today: Tween,
    pub mrr: Tween,
    pub sale: Option<(f64, &'static str)>,
    pub news: Vec<News>,
    pub apps: Vec<App>,
    /// Newest first: samples until `connect stripe` answers, then Stripe's.
    pub invoices: Vec<Invoice>,
    pub income_live: bool,
    /// Live rows from `connect x`: (meta, text, link). Empty means sample trends.
    pub posts: Vec<(String, String, String)>,
    /// A connector's last error, shown faintly on its card: (service, message).
    pub errs: Vec<(&'static str, String)>,
    /// Off in recordings: sample rows for services that are not connected stay quiet.
    pub quiet: bool,
    /// Services with a `connect` line; their sample rows and toasts stop.
    pub live: Vec<&'static str>,
    rng: u64,
    pub step: usize,
}

/// What a beat of the script wants the shell to show: (app, tint, title, body).
pub type Notice = (&'static str, u32, String, String);

impl Sim {
    pub fn new() -> Sim {
        let a = |tool: &str, repo: &str, task: &str, state, secs, ctx| Agent { tool: String::from(tool), repo: String::from(repo), cmd: String::new(), task: task.into(), doing: String::new(), state, secs, ctx, changed: -9.0 };
        let mut s = Sim {
            agents: vec![
                a("claude", "halftone", "reading src/deck.rs", State::Working, 724.0, 0.41),
                a("codex", "billing-api", "running cargo test  18/32", State::Working, 251.0, 0.22),
                a("claude", "site", "done · 3 files changed", State::Done, 1310.0, 0.67),
                a("codex", "infra", "planning terraform changes", State::Working, 96.0, 0.12),
                a("claude", "mobile", "fixing flaky login test", State::Working, 1888.0, 0.58),
            ],
            tickers: vec![], rev: vec![], today: Tween::new(1284.0, 0.8), mrr: Tween::new(0.0, 0.8), sale: None,
            news: vec![
                News { topic: "Science", head: "Eclipse photos are taking over timelines", posts: 48_200.0, at: -99.0 },
                News { topic: "Technology", head: "A new Rust release is out today", posts: 21_700.0, at: -99.0 },
                News { topic: "Space", head: "Starship Flight 15 targets a double catch", posts: 96_400.0, at: -99.0 },
                News { topic: "Business", head: "Markets open higher on chip demand", posts: 33_100.0, at: -99.0 },
            ],
            apps: vec![
                App { name: "Discord", tint: 0x5865f2, url: "https://discord.com/app", preview: "maya: pushed the new copy".into(), unread: Tween::new(3.0, 0.5), at: -99.0 },
                App { name: "Gmail", tint: 0xea4335, url: "https://mail.google.com", preview: "Your receipt from Linear".into(), unread: Tween::new(12.0, 0.5), at: -99.0 },
            ],
            posts: vec![],
            invoices: vec![],
            income_live: false,
            quiet: false,
            errs: vec![],
            live: vec![],
            rng: 0x9e37_79b9_7f4a_7c15,
            step: 0,
        };
        s.set_watch(&[], false);
        let t0 = now_unix();
        for k in 0..18 {
            let plan = ["Pro plan", "Team plan", "Pro plan", "Lifetime", "Pro plan", "Pro plan · refund"][k % 6];
            let amount = match plan { "Team plan" => 99.0, "Lifetime" => 249.0, "Pro plan · refund" => -29.0, _ => 29.0 };
            let status = if plan.ends_with("refund") { "refunded" } else if k == 4 { "open" } else { "paid" };
            s.invoices.push(Invoice { ts: t0 - 1500.0 - k as f64 * 9_700.0, who: CUSTOMERS[k % CUSTOMERS.len()].into(), what: plan.into(), amount, status: status.into(), url: String::new() });
        }
        let mut v = 38_000.0;
        for _ in 0..30 {
            v *= 1.0 + 0.004 + s.noise() * 0.012;
            s.rev.push(v);
        }
        s.mrr.snap(v);
        s
    }

    /// The watchlist: sample walks for recordings, empty rows that fill as live quotes arrive.
    pub fn set_watch(&mut self, syms: &[String], live: bool) {
        let def = ["TSLA".to_string(), "NVDA".to_string()];
        let syms = if syms.is_empty() { &def[..] } else { syms };
        self.tickers.clear();
        for (k, sym) in syms.iter().take(8).enumerate() {
            let name = match sym.as_str() { "TSLA" => "Tesla", "NVDA" => "Nvidia", "AAPL" => "Apple", "MSFT" => "Microsoft", "AMZN" => "Amazon", "META" => "Meta", "GOOGL" => "Alphabet", _ => sym }.to_string();
            if live {
                self.tickers.push(Ticker { sym: sym.clone(), name, open: 0.0, hist: vec![], vel: 0.0, price: Tween::new(0.0, 0.6), live: vec![None; 5] });
                continue;
            }
            let p = sym.bytes().fold(90.0, |a, b| a + (b % 37) as f32 * 4.0);
            let mut t = Ticker { sym: sym.clone(), name, open: p, hist: vec![p], vel: 0.0, price: Tween::new(p, 0.6), live: vec![] };
            for _ in 0..239 {
                self.walk(&mut t);
            }
            // Sample day: alternate names up and down, so both colours show.
            t.open = t.hist[60] * if k % 2 == 1 { 1.02 } else { 0.99 };
            t.price.snap(*t.hist.last().unwrap());
            self.tickers.push(t);
        }
    }

    /// Stripe's invoices replace the samples; the card's numbers come from them.
    pub fn set_invoices(&mut self, v: Vec<Invoice>) {
        self.income_live = true;
        let now = now_unix();
        let paid = |i: &&Invoice| i.status == "paid";
        let day = |i: &Invoice| ((now - i.ts) / 86_400.0) as usize;
        let mut days = vec![0.0f32; 30];
        for i in v.iter().filter(paid).filter(|i| day(i) < 30) {
            days[29 - day(i)] += i.amount;
        }
        // The chart is the running total over the 30 days.
        self.rev = days.iter().scan(0.0, |a, d| { *a += d; Some(*a) }).collect();
        self.mrr.set(*self.rev.last().unwrap());
        self.today.set(v.iter().filter(paid).filter(|i| now - i.ts < 86_400.0).map(|i| i.amount).sum());
        self.invoices = v;
    }

    /// A live quote for one range of one symbol.
    pub fn quote(&mut self, sym: &str, range: usize, q: super::quotes::Quote) {
        let Some(t) = self.tickers.iter_mut().find(|t| t.sym == sym && !t.live.is_empty()) else { return };
        if range == 0 {
            let first = t.hist.is_empty();
            t.open = q.base;
            t.hist = if q.closes.len() > 1 { q.closes.clone() } else { vec![q.base, q.price] };
            if first { t.price.snap(q.price) } else { t.price.set(q.price) }
            if !q.name.is_empty() {
                t.name = q.name.clone();
            }
        }
        t.live[range] = Some(q);
    }

    /// Marks services as connected: their rows drop the sample data until the feed answers.
    pub fn set_live(&mut self, live: Vec<&'static str>) {
        for a in &mut self.apps {
            if live.contains(&a.name) {
                a.unread.snap(0.0);
                a.preview = "connecting…".into();
            } else if self.quiet {
                a.unread.snap(0.0);
                a.preview = "not linked · press enter to set up".into();
            }
        }
        self.live = live;
        self.errs.clear();
        self.posts.clear();
    }

    fn noise(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 20_001) as f32 / 10_000.0 - 1.0
    }

    fn walk(&mut self, t: &mut Ticker) {
        t.vel = t.vel * 0.85 + self.noise() * 0.15;
        let last = *t.hist.last().unwrap();
        // Pulled back toward the open, so a long run never drifts off into +40% days.
        t.hist.push(last * (1.0 + t.vel * 0.004 + (t.open / last - 1.0) * 0.01));
    }

    pub fn tick(&mut self, dt: f64) {
        let dtf = dt as f32;
        for a in &mut self.agents {
            if a.state == State::Working && a.cmd.is_empty() {
                a.secs += dt;
                a.ctx = (a.ctx + dtf * 0.003).min(0.95);
            }
        }
        for t in &mut self.tickers {
            t.price.step(dtf);
        }
        for a in &mut self.apps {
            a.unread.step(dtf);
        }
        self.today.step(dtf);
        self.mrr.step(dtf);
    }

    /// One beat of the script. Returns a notice for the toast stack, if this beat has one.
    pub fn beat(&mut self, now: f64) -> Option<Notice> {
        self.step += 1;
        let mut ts = std::mem::take(&mut self.tickers);
        for t in ts.iter_mut().filter(|t| t.live.is_empty()) {
            self.walk(t);
            t.hist.remove(0);
            t.price.set(*t.hist.last().unwrap());
        }
        self.tickers = ts;
        for (i, n) in self.news.iter_mut().enumerate() {
            n.posts *= 1.0 + 0.002 * (4 - i.min(3)) as f32;
        }
        let s = self.step;
        let mut notice = None;
        let ag = &mut self.agents;
        // Real sessions replace the sample rows; the script leaves them alone.
        let scripted = ag.len() >= 5 && ag[0].cmd.is_empty();
        match s {
            3 if !self.quiet && !self.live.contains(&"Discord") => {
                self.apps[0].unread.set(4.0);
                self.apps[0].preview = "maya: can you look before 5?".into();
                self.apps[0].at = now;
                notice = Some(("Discord", 0x5865f2, "Maya Chen · #design".into(), "can you look at the onboarding copy before 5?".into()));
            }
            4 if scripted => ag[0].task = "editing src/deck.rs".into(),
            6 if scripted => {
                ag[3].task = "needs approval · terraform apply".into();
                ag[3].state = State::Waiting;
                ag[3].changed = now;
                notice = Some(("Codex", 0x10a37f, "infra needs approval".into(), "terraform apply · 3 to add, 1 to change".into()));
            }
            9 if !self.quiet && !self.live.contains(&"Gmail") => {
                self.apps[1].unread.set(13.0);
                self.apps[1].preview = "Stripe: payout on its way".into();
                self.apps[1].at = now;
                notice = Some(("Gmail", 0xea4335, "Stripe".into(), "Your payout of $8,412.20 is on its way".into()));
            }
            11 if scripted => ag[0].task = "running cargo test".into(),
            12 => {
                self.news.insert(0, News { topic: "Breaking", head: "Major cloud outage, services recovering", posts: 8_900.0, at: now });
                self.news.truncate(5);
            }
            15 if !self.live.contains(&"X") => notice = Some(("X", 0xe7e9ea, "Trending in Space".into(), "Starship Flight 15 targets a double catch".into())),
            17 if scripted => {
                ag[0].task = "done · 32 tests pass".into();
                ag[0].state = State::Done;
                ag[0].changed = now;
                notice = Some(("Claude", 0xd97757, "halftone is done".into(), "32 tests pass · 4 files changed".into()));
            }
            21 => notice = Some(("Chrome", 0x8ab4f8, "Download complete".into(), "q3-report.pdf · 2.4 MB".into())),
            _ => {}
        }
        if scripted && ag[1].state == State::Working {
            let n = (18 + s * 2 / 3).min(32);
            ag[1].task = if n == 32 { "done · 32/32 tests pass".into() } else { format!("running cargo test  {n}/32") };
            if n == 32 {
                ag[1].state = State::Done;
                ag[1].changed = now;
            }
        }
        if s % 5 == 2 {
            let plan = ["Pro plan", "Team plan", "Pro plan", "Lifetime"][(s / 5) % 4];
            let amt = match plan { "Team plan" => 99.0, "Lifetime" => 249.0, _ => 29.0 };
            self.today.set(self.today.target() + amt);
            *self.rev.last_mut().unwrap() += amt;
            self.mrr.set(*self.rev.last().unwrap());
            self.sale = Some((now, plan));
            if !self.income_live {
                let who = CUSTOMERS[(s * 3) % CUSTOMERS.len()].to_string();
                self.invoices.insert(0, Invoice { ts: now_unix(), who, what: plan.into(), amount: amt, status: "paid".into(), url: String::new() });
            }
        }
        notice
    }
}

/// A row that slides in from the right and brightens from the ground when it is new.
fn enter(now: f64, at: f64) -> (u16, f32) {
    let k = since(now, at, 0.45);
    (((1.0 - k) * 6.0) as u16, 1.0 - k)
}

pub fn agents(f: &mut Frame, c: Rect, sim: &Sim, sel: Option<usize>, sel_y: f32, id: u32) {
    let spin = SPIN[(f.time * 5.5) as usize % SPIN.len()];
    if let Some(_) = sel {
        // The selection bar glides between rows instead of jumping.
        let y = c.y + (sel_y * 2.0).round() as u16;
        if y < c.bottom() {
            f.buf.fill(Rect::new(c.x.saturating_sub(1), y, c.w + 2, 1), theme::raised());
        }
    }
    for (i, a) in sim.agents.iter().enumerate() {
        let y = c.y + i as u16 * 2;
        if y >= c.bottom() {
            break;
        }
        let row = Rect::new(c.x, y, c.w, 1);
        let flash = (1.0 - since(f.time, a.changed, 1.6)).max(0.0);
        let (glyph, col) = match a.state {
            State::Working => (spin, theme::accent()),
            State::Waiting => ("●  ", theme::warn()),
            State::Done => ("✓  ", theme::good()),
        };
        let x = f.buf.print(c.x, y, glyph, fg(col), 3) + 1;
        let x = f.buf.print(x, y, &a.tool, fg(theme::text()).bold(), 8).max(c.x + 13);
        let x = f.buf.print(x, y, &a.repo, fg(theme::muted()), 12).max(c.x + 26);
        let meta = clock(a.secs);
        let mx = c.right().saturating_sub(len(&meta));
        f.buf.print(mx, y, &meta, fg(theme::faint()), len(&meta));
        let gx = mx.saturating_sub(10);
        if a.ctx > 0.0 {
            meter(f.buf, gx, y, 8, a.ctx, theme::muted());
        }
        let task_col = match a.state {
            State::Waiting => theme::warn(),
            State::Done => theme::muted().mix(theme::good(), flash),
            _ => theme::text(),
        };
        let w = gx.saturating_sub(x + 2);
        if a.doing.is_empty() {
            crate::deck::marquee(f.buf, x, y, w, &a.task, f.time, fg(task_col));
        } else {
            // Title, then progress: the title keeps at most three fifths of the room.
            let tw = len(&a.task).min(w * 3 / 5);
            fit(f.buf, x, y, tw, &a.task, fg(theme::text()));
            let dc = match a.state { State::Working => theme::accent().mix(theme::muted(), 0.4), State::Done => theme::muted().mix(theme::good(), 0.35), State::Waiting => theme::warn() };
            fit(f.buf, x + tw + 2, y, w.saturating_sub(tw + 2), &a.doing, fg(dc));
        }
        f.hit(row, id + i as u32);
    }
}

/// Green up, red down, with the strength of the colour following the size of the move: a 0.1%
/// dip stays close to grey, 2% or more is full red.
pub fn move_col(pct: f32) -> Color {
    let k = (pct.abs() / 2.0).clamp(0.25, 1.0);
    theme::muted().mix(if pct >= 0.0 { theme::good() } else { theme::bad() }, k)
}

/// A `watch` card's layout from its argument: `2x2` is two columns by two rows. Without one, a
/// single column with a row per symbol.
pub fn grid(args: &[String], n: usize) -> (u16, u16) {
    args.iter().find_map(|a| {
        let (c, r) = a.split_once('x')?;
        Some((c.parse::<u16>().ok()?.clamp(1, 4), r.parse::<u16>().ok()?.clamp(1, 8)))
    }).unwrap_or((1, n.max(1) as u16))
}

/// Tile `i` of a `cols`×`rows` grid inside `c`.
pub fn tile(c: Rect, (cols, rows): (u16, u16), i: usize) -> Rect {
    let (cw, rh) = (c.w / cols, c.h / rows);
    let (cx, ry) = (i as u16 % cols, i as u16 / cols);
    Rect::new(c.x + cx * cw, c.y + ry * rh, cw.saturating_sub(if cx + 1 < cols { 3 } else { 0 }), rh.saturating_sub(1).max(1))
}

pub fn watch(f: &mut Frame, c: Rect, sim: &Sim, layout: (u16, u16), id: u32) {
    let n = sim.tickers.len().min((layout.0 * layout.1) as usize);
    for (i, t) in sim.tickers.iter().take(n).enumerate() {
        let r = tile(c, layout, i);
        f.hit(r, id + i as u32);
        let x = f.buf.print(r.x, r.y, &t.sym, fg(theme::text()).bold(), 6);
        if t.hist.is_empty() {
            f.buf.print(x + 1, r.y, "loading…", fg(theme::faint()), r.w);
            continue;
        }
        let last = t.price.get();
        let ch = (last / t.open - 1.0) * 100.0;
        let col = move_col(ch);
        let pct = format!("{}{:.2}%", if ch >= 0.0 { "+" } else { "" }, ch);
        let price = format!("{last:.2}");
        let px = r.right().saturating_sub(len(&pct) + 2 + len(&price));
        f.buf.print(r.right().saturating_sub(len(&pct)), r.y, &pct, fg(col), len(&pct));
        f.buf.print(px, r.y, &price, fg(theme::text()), len(&price));
        if px > x + 4 {
            f.buf.print(x + 1, r.y, &t.name, fg(theme::faint()), px - x - 3);
        }
        let cr = Rect::new(r.x, r.y + 1, r.w, r.h.saturating_sub(1).min(8));
        candles(f.buf, cr, &bars(t, 0), Some(t.open));
    }
}

/// Bars for a range: Yahoo's own once fetched, else the close series cut into groups of four.
pub fn bars(t: &Ticker, range: usize) -> Vec<[f32; 4]> {
    if let Some(Some(q)) = t.live.get(range) {
        if q.ohlc.len() > 1 {
            return q.ohlc.clone();
        }
    }
    let s = series(t, range);
    s.chunks(4).filter(|c| !c.is_empty()).enumerate().map(|(i, c)| {
        let open = if i == 0 { c[0] } else { s[i * 4 - 1] };
        let hi = c.iter().cloned().fold(open, f32::max);
        let lo = c.iter().cloned().fold(open, f32::min);
        [open, hi, lo, *c.last().unwrap()]
    }).collect()
}

/// Bars merged so that `n` of them remain: first open, highest high, lowest low, last close.
pub fn merge(bars: &[[f32; 4]], n: usize) -> Vec<[f32; 4]> {
    if bars.len() <= n || n == 0 {
        return bars.to_vec();
    }
    (0..n).map(|i| {
        let g = &bars[i * bars.len() / n..((i + 1) * bars.len() / n).max(i * bars.len() / n + 1)];
        [g[0][0], g.iter().map(|b| b[1]).fold(f32::MIN, f32::max), g.iter().map(|b| b[2]).fold(f32::MAX, f32::min), g[g.len() - 1][3]]
    }).collect()
}

/// Candlesticks: green where a bar closed above its open, red below. Bodies use half cells,
/// wicks a thin rule. The candles fill the height; `base` (yesterday's close) shows as a dotted
/// line only when it falls inside their range.
pub fn candles(b: &mut Buffer, r: Rect, bars: &[[f32; 4]], base: Option<f32>) {
    if bars.is_empty() || r.w < 2 || r.h == 0 {
        return;
    }
    let bars = merge(bars, (r.w as usize / 2).max(1));
    let hi = bars.iter().map(|b| b[1]).fold(f32::MIN, f32::max);
    let lo = bars.iter().map(|b| b[2]).fold(f32::MAX, f32::min);
    let span = (hi - lo).max(hi * 1e-4);
    let sub = (r.h * 2) as f32;
    let y = |v: f32| (((hi - v) / span) * (sub - 1.0)).round() as i32;
    if let Some(v) = base.filter(|v| *v >= lo && *v <= hi) {
        let row = r.y + (y(v) / 2) as u16;
        for x in (r.x..r.right()).step_by(2) {
            b.set(x, row.min(r.bottom() - 1), '·', fg(theme::faint()));
        }
    }
    let step = if bars.len() * 2 <= r.w as usize { 2 } else { 1 };
    for (i, [o, h, l, c]) in bars.iter().enumerate() {
        let x = r.x + (i * step) as u16;
        if x >= r.right() {
            break;
        }
        let col = if c >= o { theme::good() } else { theme::bad() };
        let (bt, bb) = (y(o.max(*c)), y(o.min(*c)));
        let (wt, wb) = (y(*h), y(*l));
        for k in 0..r.h as i32 {
            let (top, bot) = (k * 2, k * 2 + 1);
            let body = |s: i32| s >= bt && s <= bb;
            let wick = |s: i32| s >= wt && s <= wb;
            let ch = match (body(top), body(bot)) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                _ if wick(top) && wick(bot) => '│',
                _ if wick(top) => '╵',
                _ if wick(bot) => '╷',
                _ => continue,
            };
            b.set(x, r.y + k as u16, ch, fg(col));
        }
    }
}

/// A price line against its baseline, which stays in frame as a dotted rule: the line is green
/// when it ends above where it started, red when below.
pub fn base_chart(b: &mut Buffer, r: Rect, data: &[f32], base: f32, back: Color) {
    if data.len() < 2 || r.h == 0 {
        return;
    }
    let lo = data.iter().cloned().fold(base, f32::min);
    let hi = data.iter().cloned().fold(base, f32::max);
    let pad = (hi - lo).max(hi * 0.002) * 0.08;
    let (lo, max) = (lo - pad, hi - lo + pad * 2.0);
    let norm: Vec<f32> = data.iter().map(|v| v - lo).collect();
    let col = move_col((data.last().unwrap() / base - 1.0) * 100.0);
    chart(b, r, &norm, max, col, col.mix(back, 0.6));
    let row = r.y + ((1.0 - (base - lo) / max) * (r.h * 4 - 1) as f32 / 4.0) as u16;
    for x in (r.x..r.right()).step_by(2) {
        if let Some(c) = b.cell(x, row.min(r.bottom() - 1)) {
            if c.ch == ' ' {
                (c.ch, c.fg) = ('·', theme::faint());
            }
        }
    }
}

pub fn revenue(f: &mut Frame, c: Rect, sim: &Sim) {
    let mrr = sim.mrr.get();
    let growth = (mrr / sim.rev[0].max(1.0) - 1.0) * 100.0;
    let stats = if sim.income_live {
        let n = sim.invoices.iter().filter(|i| i.status == "paid" && now_unix() - i.ts < 30.0 * 86_400.0).count();
        [("Last 30 days", format!("${}", money(mrr)), theme::text()), ("Paid invoices", n.to_string(), theme::text()), ("Today", format!("${}", money(sim.today.get())), theme::text())]
    } else {
        [("MRR", format!("${}", money(mrr)), theme::text()), ("30 days", format!("{}{growth:.1}%", if growth >= 0.0 { "+" } else { "" }), if growth >= 0.0 { theme::good() } else { theme::bad() }), ("Today", format!("${}", money(sim.today.get())), theme::text())]
    };
    let mut x = c.x;
    for (label, v, col) in &stats {
        f.buf.print(x, c.y, label, fg(theme::faint()), 20);
        f.buf.print(x, c.y + 1, v, fg(*col).bold(), 20);
        x += len(v).max(len(label)) + 4;
    }
    if let Some((at, plan)) = sim.sale {
        let k = since(f.time, at, 2.6);
        if k < 1.0 {
            let (dx, _) = enter(f.time, at);
            let s = format!("+ {plan}");
            f.buf.print(x + dx, c.y + 1, &s, fg(theme::accent().mix(theme::panel(), k)), len(&s));
        }
    }
    let streams = [("Stripe", 0.64), ("App Store", 0.2), ("Sponsors", 0.09), ("Ads", 0.07)];
    let lw = 26.min(c.w / 3);
    let chart_r = Rect::new(c.x, c.y + 3, c.w.saturating_sub(lw + 4), c.h.saturating_sub(3));
    let lo = sim.rev.iter().cloned().fold(f32::MAX, f32::min) * 0.97;
    let norm: Vec<f32> = sim.rev.iter().map(|v| v - lo).collect();
    let max = norm.iter().cloned().fold(0.0, f32::max) * 1.05;
    chart(f.buf, chart_r, &norm, max, theme::accent(), theme::accent());
    let sx = c.right() - lw;
    let mut y = c.y + 3;
    if sim.income_live {
        // No plan split from invoices alone: the latest ones instead.
        for i in sim.invoices.iter().take(((c.h.saturating_sub(3)) / 2) as usize) {
            let v = format!("${}", money(i.amount));
            fit(f.buf, sx, y, lw.saturating_sub(len(&v) + 1), &i.who, fg(theme::muted()));
            f.buf.print(c.right() - len(&v), y, &v, fg(theme::text()), len(&v));
            y += 2;
        }
        return;
    }
    for (name, share) in streams {
        if y + 1 >= c.bottom() {
            break;
        }
        let v = format!("${}", money(mrr * share));
        f.buf.print(sx, y, name, fg(theme::muted()), lw);
        f.buf.print(c.right() - len(&v), y, &v, fg(theme::text()), len(&v));
        meter(f.buf, sx, y + 1, lw, share / 0.64, theme::blue().mix(theme::panel(), 0.25));
        y += 3;
    }
}

const CUSTOMERS: [&str; 8] = ["Northwind Studio", "Acme Labs", "Bluefin Co", "Paperplane", "Kestrel Health", "Orbital Coffee", "Lumen & Co", "Fieldnote"];

/// The income screen: the card's numbers and chart on top, every invoice below. Rows with a
/// Stripe page open it.
pub fn income(f: &mut Frame, r: Rect, sim: &Sim, id: u32) {
    let top = Rect::new(r.x, r.y, r.w, (r.h * 2 / 5).max(8));
    revenue(f, top, sim);
    let cols = [(0u16, "When"), (14, "Customer"), (40, "Item"), (r.w.saturating_sub(24), "Amount"), (r.w.saturating_sub(10), "Status")];
    let hy = top.bottom() + 1;
    for (dx, h) in cols {
        f.buf.print(r.x + dx, hy, h, fg(theme::faint()), 20);
    }
    let mut y = hy + 2;
    for (i, inv) in sim.invoices.iter().enumerate() {
        if y >= r.bottom() {
            break;
        }
        let row = Rect::new(r.x, y, r.w, 1);
        let hov = f.hover(row) && !inv.url.is_empty();
        if hov {
            f.buf.fill(Rect::new(r.x.saturating_sub(1), y, r.w + 2, 1), theme::raised());
        }
        let amt = format!("{}${}", if inv.amount < 0.0 { "−" } else { "" }, money(inv.amount.abs()));
        let sc = match inv.status.as_str() { "paid" => theme::good(), "open" | "draft" => theme::warn(), "refunded" => theme::bad(), _ => theme::faint() };
        f.buf.print(r.x, y, &ago(inv.ts), fg(theme::faint()), 13);
        fit(f.buf, r.x + 14, y, 24, &inv.who, fg(theme::text()));
        fit(f.buf, r.x + 40, y, cols[3].0.saturating_sub(42), &inv.what, fg(theme::muted()));
        f.buf.print(r.x + cols[4].0 - 2 - len(&amt), y, &amt, fg(theme::text()).bold(), 14);
        f.buf.print(r.x + cols[4].0, y, &inv.status, fg(sc), 10);
        if !inv.url.is_empty() {
            f.hit(row, id + i as u32);
        }
        y += 2;
    }
}

/// The X card. `live` is false when X is not connected outside a recording: then it says how to
/// connect instead of showing sample trends. Every row, and the card itself, opens on x.com.
pub fn news(f: &mut Frame, c: Rect, sim: &Sim, live: bool, id: u32) {
    let mut y = c.y;
    f.hit(c, id);
    if !sim.posts.is_empty() {
        for (i, (meta, text, _)) in sim.posts.iter().enumerate() {
            if y + 2 > c.bottom() {
                break;
            }
            let r = Rect::new(c.x, y, c.w, 2);
            let hov = f.hover(r);
            f.buf.print(c.x, y, meta, fg(theme::faint()), c.w);
            fit(f.buf, c.x, y + 1, c.w, text, fg(if hov { theme::accent() } else { theme::text() }));
            f.hit(r, id + 1 + i as u32);
            y += 3;
        }
        return;
    }
    if !live {
        f.buf.print(c.x, y, "Not connected", fg(theme::text()), c.w);
        f.buf.print(c.x, y + 1, "press enter to set up", fg(theme::muted()), c.w);
        return;
    }
    for n in &sim.news {
        if y + 2 > c.bottom() {
            break;
        }
        let (dx, dim) = enter(f.time, n.at);
        let breaking = n.at >= 0.0;
        let m = |col: Color| col.mix(theme::panel(), dim);
        let tc = if breaking { theme::bad().mix(theme::muted(), 0.25) } else { theme::faint() };
        let x = c.x + dx;
        let w = c.w.saturating_sub(dx);
        f.buf.print(x, y, &format!("{} · {} posts", n.topic, short(n.posts)), fg(m(tc)), w);
        crate::deck::marquee(f.buf, x, y + 1, w, n.head, f.time, fg(m(theme::text())));
        y += 3;
    }
}

/// The `start` card: one row per (name, hint, done), then a last row that hides the card.
pub fn start(f: &mut Frame, c: Rect, rows: &[(String, String, bool)], sel: Option<usize>, id: u32) {
    for (i, (name, hint, done)) in rows.iter().map(|r| (r.0.as_str(), r.1.as_str(), r.2)).chain([("Hide this card", "", false)]).enumerate() {
        let y = c.y + i as u16;
        if y >= c.bottom() {
            break;
        }
        let r = Rect::new(c.x, y, c.w, 1);
        let on = sel == Some(i) || f.hover(r);
        if i < rows.len() {
            f.buf.print(c.x, y, if done { "✓" } else { "○" }, fg(if done { theme::good() } else { theme::faint() }), 1);
        }
        let x = f.buf.print(c.x + 2, y, name, fg(if on { theme::text() } else { theme::muted() }).bold(), c.w.saturating_sub(2));
        if !hint.is_empty() {
            fit(f.buf, x + 1, y, c.right().saturating_sub(x + 1), &format!("· {hint}"), fg(if done { theme::muted() } else { theme::faint() }));
        }
        if sel == Some(i) {
            f.buf.set(c.x.saturating_sub(2), y, '▎', fg(theme::accent()));
        }
        f.hit(r, id + i as u32);
    }
}

pub fn apps(f: &mut Frame, c: Rect, sim: &Sim, id: u32) {
    for (i, a) in sim.apps.iter().enumerate() {
        let cy = c.y + i as u16;
        if cy >= c.bottom() {
            break;
        }
        let r = Rect::new(c.x, cy, c.w, 1);
        let hot = (1.0 - since(f.time, a.at, 2.0)).max(0.0);
        let t = tone(a.tint);
        f.buf.set(c.x, cy, '●', fg(t));
        f.buf.print(c.x + 2, cy, a.name, fg(theme::text()).bold(), 10);
        let n = a.unread.get().round() as u32;
        let badge = if n > 0 { n.to_string() } else { String::new() };
        let bx = r.right().saturating_sub(len(&badge));
        f.buf.print(bx, cy, &badge, fg(theme::muted().mix(theme::text(), hot)).bold(), len(&badge));
        let px = c.x + 11;
        let err = sim.errs.iter().find(|(s, _)| *s == a.name).map(|(_, e)| e.as_str());
        let (text, col) = match err { Some(e) => (e, theme::bad().mix(theme::faint(), 0.5)), None => (a.preview.as_str(), theme::muted().mix(theme::text(), hot)) };
        fit(f.buf, px, cy, bx.saturating_sub(px + 2), text, fg(col));
        f.hit(r, id + i as u32);
    }
}

/// Prints `s` in at most `w` cells, ending in an ellipsis when it had to cut.
pub fn fit(b: &mut Buffer, x: u16, y: u16, w: u16, s: &str, st: Style) -> u16 {
    if len(s) <= w {
        return b.print(x, y, s, st, w);
    }
    let cut: String = s.chars().take(w.saturating_sub(1) as usize).collect();
    let nx = b.print(x, y, cut.trim_end(), st, w);
    b.print(nx, y, "…", st, 1)
}

/// Stock detail: a big braille chart that morphs between ranges, the day's numbers, related posts.
pub fn stock(f: &mut Frame, r: Rect, t: &Ticker, range: usize, morph: f32, prev: &[f32]) {
    if t.hist.is_empty() {
        let x = f.buf.print(r.x, r.y, &t.sym, fg(theme::text()).bold(), 8);
        f.buf.print(x + 2, r.y, "loading quotes…", fg(theme::faint()), 30);
        return;
    }
    let last = t.price.get();
    let ch = (last / t.open - 1.0) * 100.0;
    let col = move_col(ch);
    let x = f.buf.print(r.x, r.y, &t.sym, fg(theme::text()).bold(), 8);
    f.buf.print(x + 2, r.y, &t.name, fg(theme::muted()), 30);
    let price = format!("${last:.2}");
    f.buf.print(r.x, r.y + 2, &price, fg(theme::text()).bold(), 20);
    let pct = format!("{}{:.2}%  today", if ch >= 0.0 { "▲ " } else { "▼ " }, ch.abs());
    f.buf.print(r.x + len(&price) + 2, r.y + 2, &pct, fg(col), 30);
    let side = 34.min(r.w / 3);
    let cw = r.w.saturating_sub(side + 4);
    // Ranges as text tabs; the active one is underlined by a hairline that the shell animates.
    let mut tx = r.x;
    for (i, name) in RANGES.iter().enumerate() {
        let st = if i == range { fg(theme::text()).bold() } else { fg(theme::faint()) };
        tx = f.buf.print(tx, r.y + 4, name, st, 4) + 3;
        f.hit(Rect::new(tx - 3 - len(name), r.y + 4, len(name), 1), 900 + i as u32);
    }
    let data = series(t, range);
    let n = data.len();
    let shown: Vec<f32> = (0..n).map(|i| {
        let p = prev.get(i * prev.len().max(1) / n.max(1)).copied().unwrap_or(data[i]);
        p + (data[i] - p) * morph
    }).collect();
    // The day is measured from yesterday's close; longer ranges from where they start.
    let base = match t.live.get(range) {
        Some(Some(q)) => q.base,
        _ if range == 0 => t.open,
        _ => data[0],
    };
    let _ = (col, &shown);
    candles(f.buf, Rect::new(r.x, r.y + 6, cw, r.h.saturating_sub(7)), &bars(t, range), Some(base));
    let sx = r.right() - side;
    let hi = t.hist.iter().cloned().fold(f32::MIN, f32::max);
    let low = t.hist.iter().cloned().fold(f32::MAX, f32::min);
    if let Some(Some(q)) = t.live.first() {
        let vol = if q.volume >= 1e9 { format!("{:.1}B", q.volume / 1e9) } else { format!("{:.1}M", q.volume / 1e6) };
        let rows = [
            ("Prev close", format!("{:.2}", q.base)),
            ("High", format!("{:.2}", q.high)),
            ("Low", format!("{:.2}", q.low)),
            ("Volume", vol),
            ("52w range", format!("{:.0} – {:.0}", q.year.0, q.year.1)),
        ];
        let mut y = r.y + 6;
        for (k, v) in rows {
            f.buf.print(sx, y, k, fg(theme::faint()), side);
            f.buf.print(r.right() - len(&v), y, &v, fg(theme::text()), len(&v));
            y += 2;
        }
        let link = format!("${} on X  ↗", t.sym);
        let lr = Rect::new(sx, y + 1, len(&link), 1);
        let st = if f.hover(lr) { fg(theme::text()) } else { fg(theme::muted()) };
        f.buf.print(sx, y + 1, &link, st, side);
        f.hit(lr, 1100);
        f.buf.print(sx, r.bottom().saturating_sub(1), "Yahoo Finance · live", fg(theme::faint()), side);
        return;
    }
    let rows = [
        ("Open", format!("{:.2}", t.open)),
        ("High", format!("{hi:.2}")),
        ("Low", format!("{low:.2}")),
        ("Volume", "84.2M".into()),
        ("Market cap", if t.sym == "TSLA" { "812B".into() } else { "3.4T".into() }),
        ("52w range", format!("{:.0} – {:.0}", low * 0.71, hi * 1.18)),
    ];
    let mut y = r.y + 6;
    for (k, v) in rows {
        f.buf.print(sx, y, k, fg(theme::faint()), side);
        f.buf.print(r.right() - len(&v), y, &v, fg(theme::text()), len(&v));
        y += 2;
    }
    y += 1;
    f.buf.print(sx, y, &format!("On X · ${}", t.sym), fg(theme::muted()).bold(), side);
    let posts: &[&str] = if t.sym == "TSLA" {
        &["Roadster demo clips are everywhere", "Deliveries beat estimates", "Robotaxi expands to two cities"]
    } else {
        &["New chips sold out through Q2", "Data center orders keep climbing", "Analysts lift targets again"]
    };
    for p in posts {
        y += 2;
        if y >= r.bottom() {
            break;
        }
        f.buf.print(sx, y, p, fg(theme::text()), side);
    }
}

pub const RANGES: [&str; 5] = ["1D", "5D", "1M", "6M", "1Y"];

/// The ticker's history over a range. Short ranges are the tail; longer ones extend it backward
/// with a seeded walk (same symbol, same shape every run), sampled to the same point count.
pub fn series(t: &Ticker, range: usize) -> Vec<f32> {
    if !t.live.is_empty() {
        // Live: the range's closes once fetched, the day's line until then.
        return match t.live.get(range) {
            Some(Some(q)) if q.closes.len() > 1 => q.closes.clone(),
            _ => t.hist.clone(),
        };
    }
    let h = &t.hist;
    let days = [60, 150, 600, 1800, 3600][range.min(4)];
    let mut back: Vec<f32> = h.clone();
    let mut seed = t.sym.bytes().fold(0x2545_f491_4f6c_dd1du64, |a, b| a.rotate_left(5) ^ b as u64);
    let mut vel = 0.0f32;
    while back.len() < days {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let n = (seed % 20_001) as f32 / 10_000.0 - 1.0;
        vel = vel * 0.9 + n * 0.1;
        let first = back[0];
        back.insert(0, first / (1.0 + vel * 0.006 + 0.0006));
    }
    let tail = &back[back.len() - days..];
    let n = 180.min(tail.len());
    (0..n).map(|i| tail[i * tail.len() / n]).collect()
}

/// Now playing as a strip: play button, title and artist on one line, a hairline rail under it.
/// The whole strip opens the full player; the button and rail keep their own clicks.
pub fn media_card(f: &mut Frame, c: Rect, title: &str, artist: &str, pos: f64, dur: Option<f64>, playing: bool, ids: (u32, u32, u32)) {
    f.hit(c, ids.0);
    let play = if playing { "⏸" } else { "▶" };
    f.buf.print(c.x, c.y, play, fg(theme::text()).bold(), 1);
    f.hit(Rect::new(c.x, c.y, 2, 1), ids.1);
    let tx = c.x + 3;
    let time = format!("{} / {}", crate::media::clock(pos), dur.map_or("live".into(), crate::media::clock));
    let tw = c.w.saturating_sub(len(&time) + 6);
    let ex = crate::deck::marquee(f.buf, tx, c.y, tw * 2 / 3, title, f.time, fg(theme::text()).bold());
    if !artist.is_empty() {
        fit(f.buf, ex + 2, c.y, (tx + tw).saturating_sub(ex + 2), artist, fg(theme::muted()));
    }
    f.buf.print(c.right() - len(&time), c.y, &time, fg(theme::faint()), len(&time));
    if c.h > 1 {
        let y = c.y + 1;
        let v = dur.map_or(0.0, |l| (pos / l.max(1e-3)) as f32);
        meter(f.buf, tx, y, c.w - 3, v, theme::accent().mix(theme::text(), 0.15));
        f.hit(Rect::new(tx, y, c.w - 3, 1), ids.2);
    }
}

/// Tints a rect toward `c` by `k`, keeping glyphs. Used for the media card's cover-colour wash.
pub fn wash(b: &mut Buffer, r: Rect, c: Color, k: f32) {
    for y in r.y..r.bottom() {
        for x in r.x..r.right() {
            if let Some(cell) = b.cell(x, y) {
                cell.bg = cell.bg.mix(c, k);
            }
        }
    }
}

fn mod_tone(t: Option<&str>) -> Color {
    match t {
        Some("good") => theme::good(),
        Some("bad") => theme::bad(),
        Some("warn") => theme::warn(),
        Some("muted") => theme::muted(),
        Some("accent") => theme::accent(),
        _ => theme::text(),
    }
}

/// A script module's last good output; `id` is the card's hit, its links take id + 1.. in order.
/// Clickable targets are pushed to `hits` as (id, url or command).
pub fn module(f: &mut Frame, c: Rect, slot: &super::modules::Slot, id: u32, hits: &mut Vec<(u32, String)>) {
    use super::modules::Line;
    let Some(out) = &slot.out else {
        let (m, col) = match &slot.err {
            Some(e) => (e.as_str(), theme::bad().mix(theme::faint(), 0.5)),
            None => ("loading…", theme::faint()),
        };
        fit(f.buf, c.x, c.y, c.w, m, fg(col));
        return;
    };
    f.hit(c, id);
    let mut c = c;
    // Enter on the card opens the connect view if it asks for one, else `open:`, else the first link.
    if let Some((name, url, prompt)) = &out.connect {
        hits.push((id, format!("\u{1}{name}\t{url}\t{prompt}")));
        fit(f.buf, c.x, c.y, c.w, &format!("Connect {} · press enter", super::modules::cap(name)), fg(theme::accent()).bold());
        fit(f.buf, c.x, c.y + 1, c.w, &format!("needs {prompt}"), fg(theme::faint()));
        c = Rect::new(c.x, c.y + 3, c.w, c.h.saturating_sub(3));
    } else if let Some(u) = out.open.clone().or_else(|| out.lines.iter().find_map(|l| if let Line::Link(_, u, _) = l { Some(u.clone()) } else { None })) {
        hits.push((id, u));
    }
    // Room to breathe: when everything fits at two rows a line, use two, like the agents card.
    let need: u16 = out.lines.iter().map(|l| if matches!(l, Line::Head(_) | Line::Big(_)) { 2 } else { 1 }).sum();
    let pitch = if need * 2 <= c.h { 2 } else { 1 };
    let mut y = c.y;
    let mut links = 0;
    for (i, l) in out.lines.iter().enumerate() {
        if y >= c.bottom() {
            break;
        }
        match l {
            Line::Head(s) => {
                if i > 0 {
                    y += 1;
                }
                if y >= c.bottom() {
                    break;
                }
                fit(f.buf, c.x, y, c.w, s, fg(theme::faint()));
                y += 1;
                continue;
            }
            Line::Big(s) => _ = fit(f.buf, c.x, y, c.w, s, fg(theme::text()).bold()),
            Line::Row(label, value, tone) => {
                let vx = c.right().saturating_sub(len(value));
                f.buf.print(vx, y, value, fg(mod_tone(tone.as_deref())), len(value));
                fit(f.buf, c.x, y, vx.saturating_sub(c.x + 1), label, fg(theme::muted()));
            }
            Line::Bar(label, v, tone) => {
                let pct = format!("{:>3}%", (v * 100.0).round() as u32);
                f.buf.print(c.right().saturating_sub(4), y, &pct, fg(theme::muted()), 4);
                let lw = (c.w / 2).min(len(label) + 1).max(1);
                fit(f.buf, c.x, y, lw, label, fg(theme::muted()));
                let mx = c.x + lw;
                let mw = c.right().saturating_sub(5).saturating_sub(mx);
                if mw > 0 {
                    meter(f.buf, mx, y, mw, *v, mod_tone(Some(tone.as_deref().unwrap_or("accent"))));
                }
            }
            Line::Spark(d) => {
                let (lo, hi) = d.iter().fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
                let n: Vec<f32> = d.iter().map(|v| (v - lo) / (hi - lo).max(1e-6)).collect();
                crate::widgets::sparkline(f.buf, Rect::new(c.x, y, c.w, 1), &n, 1.0, theme::accent());
            }
            Line::Link(label, url, note) => {
                let r = Rect::new(c.x, y, c.w, 1);
                let hov = f.hover(r);
                let nx = c.right().saturating_sub(len(note));
                f.buf.print(nx, y, note, fg(theme::faint()), len(note));
                fit(f.buf, c.x, y, nx.saturating_sub(c.x + 2), label, fg(if hov { theme::accent() } else { theme::text() }));
                if links < 31 {
                    links += 1;
                    f.hit(r, id + links);
                    hits.push((id + links, url.clone()));
                }
            }
            Line::Text(s) => _ = fit(f.buf, c.x, y, c.w, s, fg(theme::muted())),
            Line::Error(s) => _ = fit(f.buf, c.x, y, c.w, s, fg(theme::bad().mix(theme::faint(), 0.5))),
        }
        y += if matches!(l, Line::Big(_)) { 2 } else { pitch };
    }
    // A failed refresh keeps the last good lines and says why, dimly, underneath.
    if let (Some(e), true) = (&slot.err, y < c.bottom()) {
        fit(f.buf, c.x, y, c.w, &format!("stale · {e}"), fg(theme::faint()));
    }
}
