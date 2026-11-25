//! Live feeds behind the cards, from tokens in the desk file:
//!   connect gmail $GMAIL_TOKEN            (OAuth access token, gmail.readonly)
//!   connect discord $DISCORD_BOT CHANNEL  (bot token and a channel id)
//!   connect x @signin                     (after `halftone connect x`; or a token and user id)
//! Each runs on its own thread through curl; the token goes in on stdin, never on the command line.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

pub enum Update {
    /// An inbox row: app name, unread count, preview, and a toast for something new.
    Inbox { app: &'static str, unread: u32, preview: String, new: Option<(String, String)> },
    /// Invoices from Stripe, newest first.
    Invoices(Vec<super::cards::Invoice>),
    /// Rows for the X card: (meta line, text, link). Your trends first, then the most-liked posts
    /// from your timeline.
    Posts(Vec<(String, String, String)>),
    Error(&'static str, String),
}

pub struct Feeds {
    pub rx: Receiver<Update>,
}

impl Feeds {
    pub fn start(list: &[(String, Vec<String>)]) -> Feeds {
        let (tx, rx) = mpsc::channel();
        for (service, args) in list {
            let args: Vec<String> = args.iter().map(|a| env(a)).collect();
            let tx = tx.clone();
            match service.as_str() {
                // `@app` reads the desktop app's Dock badge: no token, nothing to register.
                "gmail" | "discord" if args.first().map(String::as_str) == Some("@app") => {
                    let (name, app) = if service == "gmail" { ("Gmail", "Mail") } else { ("Discord", "Discord") };
                    spawn(tx, 10, move |seen| badge(name, app, seen))
                }
                "gmail" if args.first().is_some_and(|a| a.contains('@')) => spawn(tx, 60, move |seen| imap(&args, seen)),
                "gmail" => spawn(tx, 60, move |seen| gmail(&args, seen)),
                "discord" => spawn(tx, 30, move |seen| discord(&args, seen)),
                "x" => spawn(tx, 300, move |_| x(&args)),
                "stripe" => spawn(tx, 120, move |_| stripe(&args)),
                _ => {}
            }
        }
        Feeds { rx }
    }
}

/// `$NAME` reads the environment and `@/path` a file (what `halftone setup` writes), so secrets
/// need not sit in the desk file.
fn env(a: &str) -> String {
    if let Some(p) = a.strip_prefix('@').filter(|p| p.starts_with('/')) {
        return std::fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default();
    }
    a.strip_prefix('$').map_or(a.to_string(), |k| std::env::var(k).unwrap_or_default())
}

/// One reading of a service, for `halftone setup` to check what you typed.
pub fn probe(service: &str, args: &[String]) -> Result<String, String> {
    let args: Vec<String> = args.iter().map(|a| env(a)).collect();
    let u = match service {
        "gmail" => imap(&args, &mut String::new()),
        "stripe" => stripe(&args),
        _ => return Err(format!("nothing to check for {service}")),
    };
    match u {
        Update::Inbox { unread, .. } => Ok(format!("{unread} unread")),
        Update::Invoices(v) => Ok(format!("{} recent invoices", v.len())),
        Update::Posts(p) => Ok(format!("{} posts", p.len())),
        Update::Error(_, e) => Err(e),
    }
}

fn spawn(tx: Sender<Update>, every: u64, mut f: impl FnMut(&mut String) -> Update + Send + 'static) {
    thread::spawn(move || {
        let mut seen = String::new();
        loop {
            if tx.send(f(&mut seen)).is_err() {
                return;
            }
            thread::sleep(Duration::from_secs(every));
        }
    });
}

pub fn get_json(url: &str, auth: &str) -> Result<J, String> {
    get(url, auth)
}

fn get(url: &str, auth: &str) -> Result<J, String> {
    let mut c = Command::new("curl")
        .args(["-sS", "--max-time", "15", "-w", "\n%{http_code}", "-H", "@-", url])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|_| "needs curl".to_string())?;
    c.stdin.take().ok_or("no stdin")?.write_all(format!("Authorization: {auth}\n").as_bytes()).map_err(|e| e.to_string())?;
    let o = c.wait_with_output().map_err(|e| e.to_string())?;
    let out = String::from_utf8_lossy(&o.stdout);
    let (body, code) = out.rsplit_once('\n').unwrap_or((&out, "0"));
    match code.trim() {
        "200" => J::parse(body).ok_or_else(|| "bad reply".into()),
        "401" | "403" => Err("token refused · check it in the desk file".into()),
        "429" => Err("rate limited · trying again later".into()),
        c => Err(format!("http {c}")),
    }
}

/// The number on an app's Dock icon, via `lsappinfo` (macOS). Discord's counts your mentions and
/// DMs, Mail's its unread mail.
fn badge(name: &'static str, app: &str, seen: &mut String) -> Update {
    // `-only StatusLabel` prints nothing useful on macOS 27; `-long` still includes the badge.
    let Ok(o) = Command::new("lsappinfo").args(["info", "-long", "-app", app]).output() else {
        return Update::Error(name, "needs macOS".into());
    };
    let out = String::from_utf8_lossy(&o.stdout);
    if out.trim().is_empty() {
        let hint = if app == "Mail" { "Mail is closed · enter opens it" } else { "Discord is closed · enter opens it" };
        return Update::Error(name, hint.into());
    }
    let (unread, dot) = (badge_count(&out), out.contains("\"label\"=\"•\""));
    let new = (unread > seen.parse().unwrap_or(u32::MAX)).then(|| (app.to_string(), if dot { "New messages".into() } else { format!("{unread} unread") }));
    *seen = unread.to_string();
    let preview = match (unread, app) {
        (0, "Discord") => "all caught up".into(),
        (0, _) => "inbox clear".into(),
        _ if dot => "new messages".into(),
        (n, "Discord") => format!("{n} mentions and DMs"),
        (n, _) => format!("{n} unread"),
    };
    Update::Inbox { app: name, unread, preview, new }
}

/// `"StatusLabel"={ "label"="3" }` is 3; a bare dot badge counts as 1; no badge is 0.
fn badge_count(out: &str) -> u32 {
    let Some(label) = out.split("\"label\"=\"").nth(1).and_then(|l| l.split('"').next()) else { return 0 };
    label.chars().filter(char::is_ascii_digit).collect::<String>().parse().unwrap_or(u32::from(!label.is_empty()))
}

/// `connect gmail you@example.com <app password>`: unread count over IMAP. The password goes to
/// curl on stdin, never on its command line.
fn imap(a: &[String], seen: &mut String) -> Update {
    let (Some(user), Some(pass)) = (a.first(), a.get(1).filter(|p| !p.is_empty())) else {
        return Update::Error("Gmail", "needs an app password · halftone setup gmail".into());
    };
    let Ok(mut c) = Command::new("curl").args(["-sS", "--max-time", "20", "-K", "-", "--url", "imaps://imap.gmail.com:993/INBOX", "-X", "UID SEARCH UNSEEN"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() else { return Update::Error("Gmail", "needs curl".into()) };
    let cfg = format!("user = \"{}:{}\"\n", user.replace('"', "\\\""), pass.replace(' ', "").replace('"', "\\\""));
    if let Some(mut i) = c.stdin.take() {
        let _ = i.write_all(cfg.as_bytes());
    }
    let Ok(o) = c.wait_with_output() else { return Update::Error("Gmail", "curl failed".into()) };
    if !o.status.success() {
        return Update::Error("Gmail", "sign-in refused · check the app password".into());
    }
    let uids: Vec<u64> = String::from_utf8_lossy(&o.stdout).lines().filter_map(|l| l.strip_prefix("* SEARCH")).flat_map(|l| l.split_whitespace().filter_map(|n| n.parse().ok()).collect::<Vec<_>>()).collect();
    let top = uids.iter().max().copied().unwrap_or(0);
    let new = (top > seen.parse().unwrap_or(u64::MAX)).then(|| ("Gmail".to_string(), "New mail".to_string()));
    *seen = top.to_string();
    let n = uids.len() as u32;
    Update::Inbox { app: "Gmail", unread: n, preview: if n == 0 { "Inbox zero".into() } else { format!("{n} unread") }, new }
}

fn gmail(a: &[String], seen: &mut String) -> Update {
    let Some(tok) = a.first().filter(|t| !t.is_empty()) else { return Update::Error("Gmail", "no token".into()) };
    let base = "https://gmail.googleapis.com/gmail/v1/users/me/messages";
    let auth = format!("Bearer {tok}");
    let list = match get(&format!("{base}?q=is:unread%20in:inbox&maxResults=1"), &auth) {
        Ok(j) => j,
        Err(e) => return Update::Error("Gmail", e),
    };
    let unread = list.get("resultSizeEstimate").and_then(J::num).unwrap_or(0.0) as u32;
    let Some(id) = list.get("messages").and_then(|m| m.at(0)).and_then(|m| m.get("id")).and_then(J::str).map(String::from) else {
        return Update::Inbox { app: "Gmail", unread, preview: "Inbox zero".into(), new: None };
    };
    let msg = match get(&format!("{base}/{id}?format=metadata&metadataHeaders=From&metadataHeaders=Subject"), &auth) {
        Ok(j) => j,
        Err(e) => return Update::Error("Gmail", e),
    };
    let header = |name: &str| {
        msg.get("payload").and_then(|p| p.get("headers")).and_then(J::arr).and_then(|hs| {
            hs.iter().find(|h| h.get("name").and_then(J::str) == Some(name)).and_then(|h| h.get("value")).and_then(J::str).map(String::from)
        }).unwrap_or_default()
    };
    // "Ada Lovelace <ada@example.com>" reads as "Ada Lovelace".
    let from = header("From");
    let from = from.split(" <").next().unwrap_or("").trim_matches('"').to_string();
    let subject = header("Subject");
    let new = (*seen != id && !seen.is_empty()).then(|| (from.clone(), subject.clone()));
    *seen = id;
    Update::Inbox { app: "Gmail", unread, preview: format!("{from}: {subject}"), new }
}

fn discord(a: &[String], seen: &mut String) -> Update {
    let (Some(tok), Some(chan)) = (a.first().filter(|t| !t.is_empty()), a.get(1)) else {
        return Update::Error("Discord", "needs a bot token and a channel id".into());
    };
    let j = match get(&format!("https://discord.com/api/v10/channels/{chan}/messages?limit=20"), &format!("Bot {tok}")) {
        Ok(j) => j,
        Err(e) => return Update::Error("Discord", e),
    };
    let msgs = j.arr().cloned().unwrap_or_default();
    let name = |m: &J| m.get("author").and_then(|u| u.get("global_name").and_then(J::str).or(u.get("username").and_then(J::str))).unwrap_or("someone").to_string();
    let Some(top) = msgs.first() else { return Update::Inbox { app: "Discord", unread: 0, preview: "No messages".into(), new: None } };
    let top_id = top.get("id").and_then(J::str).unwrap_or("").to_string();
    // `seen` is "first id<TAB>last id". Unread counts messages since the first poll, since
    // Discord gives bots no read state; a toast fires once per new top message.
    let (first, last) = seen.split_once('\t').map_or((String::new(), String::new()), |(a, b)| (a.to_string(), b.to_string()));
    let unread = if first.is_empty() { 0 } else { msgs.iter().take_while(|m| m.get("id").and_then(J::str) != Some(first.as_str())).count() as u32 };
    let text = top.get("content").and_then(J::str).unwrap_or("").replace('\n', " ");
    let new = (!last.is_empty() && last != top_id).then(|| (name(top), text.clone()));
    *seen = format!("{}\t{top_id}", if first.is_empty() { &top_id } else { &first });
    Update::Inbox { app: "Discord", unread, preview: format!("{}: {text}", name(top)), new }
}

fn x(a: &[String]) -> Update {
    // `@signin` uses the tokens `halftone connect x` saved, refreshed as they lapse.
    let (tok, uid) = if a.first().map(String::as_str) == Some("@signin") {
        match super::xauth::Tok::fresh() {
            Ok(t) => (t.access, t.user),
            Err(e) => return Update::Error("X", e),
        }
    } else {
        match (a.first().filter(|t| !t.is_empty()), a.get(1)) {
            (Some(t), Some(u)) => (t.clone(), u.clone()),
            _ => return Update::Error("X", "not signed in · run: halftone setup x".into()),
        }
    };
    let auth = format!("Bearer {tok}");
    // X has no API for the For You timeline; personalized trends plus your timeline ranked by
    // likes is the closest it offers.
    let trends = get("https://api.x.com/2/users/personalized_trends?personalized_trend.fields=category,post_count,trend_name", &auth);
    let tl = get(&format!("https://api.x.com/2/users/{uid}/timelines/reverse_chronological?max_results=30&expansions=author_id&user.fields=username&tweet.fields=public_metrics"), &auth);
    if let (Err(e), Err(_)) = (&trends, &tl) {
        return Update::Error("X", e.clone());
    }
    let mut rows = trends.map(|j| x_trends(&j)).unwrap_or_default();
    rows.extend(tl.map(|j| x_top(&j)).unwrap_or_default());
    Update::Posts(rows)
}

fn x_trends(j: &J) -> Vec<(String, String, String)> {
    j.get("data").and_then(J::arr).into_iter().flatten().take(3).filter_map(|t| {
        let name = t.get("trend_name").and_then(J::str)?;
        let cat = t.get("category").and_then(J::str).filter(|c| !c.is_empty()).unwrap_or("Trending");
        let meta = match t.get("post_count").and_then(J::str) {
            Some(n) if !n.is_empty() => format!("{cat} · {n} posts"),
            _ => cat.to_string(),
        };
        Some((meta, name.to_string(), format!("https://x.com/search?q={}", super::xauth::enc(name))))
    }).collect()
}

/// The timeline's posts, most liked first.
fn x_top(j: &J) -> Vec<(String, String, String)> {
    let users = j.get("includes").and_then(|i| i.get("users")).and_then(J::arr).cloned().unwrap_or_default();
    let handle = |id: &str| users.iter().find(|u| u.get("id").and_then(J::str) == Some(id)).and_then(|u| u.get("username")).and_then(J::str).unwrap_or("i").to_string();
    let mut posts: Vec<(f64, String, String, String)> = j.get("data").and_then(J::arr).into_iter().flatten().filter_map(|p| {
        let who = handle(p.get("author_id").and_then(J::str).unwrap_or(""));
        let id = p.get("id").and_then(J::str)?;
        let likes = p.get("public_metrics").and_then(|m| m.get("like_count")).and_then(J::num).unwrap_or(0.0);
        let text = p.get("text").and_then(J::str).unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ");
        Some((likes, format!("@{who} · ♥ {}", super::cards::short(likes as f32)), text, format!("https://x.com/{who}/status/{id}")))
    }).collect();
    posts.sort_by(|a, b| b.0.total_cmp(&a.0));
    posts.into_iter().map(|(_, m, t, u)| (m, t, u)).collect()
}

/// `connect stripe <key>`: the latest invoices. A restricted key with read access to invoices is
/// enough.
fn stripe(a: &[String]) -> Update {
    let Some(key) = a.first().filter(|k| !k.is_empty()) else { return Update::Error("Stripe", "no key · halftone setup stripe".into()) };
    match get("https://api.stripe.com/v1/invoices?limit=50", &format!("Bearer {key}")) {
        Ok(j) => Update::Invoices(stripe_rows(&j)),
        Err(e) => Update::Error("Stripe", e),
    }
}

fn stripe_rows(j: &J) -> Vec<super::cards::Invoice> {
    j.get("data").and_then(J::arr).into_iter().flatten().map(|i| {
        let s = |k: &str| i.get(k).and_then(J::str).unwrap_or("").to_string();
        let n = |k: &str| i.get(k).and_then(J::num).unwrap_or(0.0);
        let status = s("status");
        let cents = if status == "paid" { n("amount_paid") } else { n("amount_due") };
        let line = i.get("lines").and_then(|l| l.get("data")).and_then(|d| d.at(0)).and_then(|l| l.get("description")).and_then(J::str).unwrap_or("").to_string();
        let who = [s("customer_name"), s("customer_email")].into_iter().find(|w| !w.is_empty()).unwrap_or_else(|| "—".into());
        super::cards::Invoice { ts: n("created"), who, what: if line.is_empty() { s("number") } else { line }, amount: (cents / 100.0) as f32, status, url: s("hosted_invoice_url") }
    }).collect()
}

/// Just enough JSON for API replies.
#[derive(Clone, Debug, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    pub fn parse(s: &str) -> Option<J> {
        let b = s.as_bytes();
        let mut i = 0;
        let v = value(b, &mut i)?;
        ws(b, &mut i);
        (i == b.len()).then_some(v)
    }
    pub fn get(&self, k: &str) -> Option<&J> {
        match self { J::Obj(m) => m.iter().find(|(n, _)| n == k).map(|(_, v)| v), _ => None }
    }
    pub fn at(&self, i: usize) -> Option<&J> {
        self.arr()?.get(i)
    }
    pub fn arr(&self) -> Option<&Vec<J>> {
        match self { J::Arr(a) => Some(a), _ => None }
    }
    pub fn str(&self) -> Option<&str> {
        match self { J::Str(s) => Some(s), _ => None }
    }
    pub fn num(&self) -> Option<f64> {
        match self { J::Num(n) => Some(*n), _ => None }
    }
}

fn ws(b: &[u8], i: &mut usize) {
    while *i < b.len() && b[*i].is_ascii_whitespace() {
        *i += 1;
    }
}

fn value(b: &[u8], i: &mut usize) -> Option<J> {
    ws(b, i);
    match *b.get(*i)? {
        b'{' => {
            *i += 1;
            let mut m = vec![];
            loop {
                ws(b, i);
                if b.get(*i) == Some(&b'}') {
                    *i += 1;
                    return Some(J::Obj(m));
                }
                let J::Str(k) = value(b, i)? else { return None };
                ws(b, i);
                (b.get(*i) == Some(&b':')).then_some(())?;
                *i += 1;
                m.push((k, value(b, i)?));
                ws(b, i);
                match b.get(*i)? { b',' => *i += 1, b'}' => {} _ => return None }
            }
        }
        b'[' => {
            *i += 1;
            let mut a = vec![];
            loop {
                ws(b, i);
                if b.get(*i) == Some(&b']') {
                    *i += 1;
                    return Some(J::Arr(a));
                }
                a.push(value(b, i)?);
                ws(b, i);
                match b.get(*i)? { b',' => *i += 1, b']' => {} _ => return None }
            }
        }
        b'"' => {
            *i += 1;
            let mut out = Vec::new();
            loop {
                match *b.get(*i)? {
                    b'"' => {
                        *i += 1;
                        return Some(J::Str(String::from_utf8_lossy(&out).into_owned()));
                    }
                    b'\\' => {
                        let e = *b.get(*i + 1)?;
                        *i += 2;
                        match e {
                            b'n' => out.push(b'\n'),
                            b't' => out.push(b'\t'),
                            b'r' => {}
                            b'b' | b'f' => {}
                            b'u' => {
                                let hex = std::str::from_utf8(b.get(*i..*i + 4)?).ok()?;
                                let mut cp = u32::from_str_radix(hex, 16).ok()?;
                                *i += 4;
                                // A surrogate pair (emoji) arrives as two \u escapes.
                                if (0xd800..0xdc00).contains(&cp) && b.get(*i..*i + 2) == Some(b"\\u") {
                                    let lo = u32::from_str_radix(std::str::from_utf8(b.get(*i + 2..*i + 6)?).ok()?, 16).ok()?;
                                    cp = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
                                    *i += 6;
                                }
                                let mut buf = [0u8; 4];
                                out.extend_from_slice(char::from_u32(cp).unwrap_or('\u{fffd}').encode_utf8(&mut buf).as_bytes());
                            }
                            c => out.push(c),
                        }
                    }
                    c => {
                        out.push(c);
                        *i += 1;
                    }
                }
            }
        }
        b't' if b[*i..].starts_with(b"true") => {
            *i += 4;
            Some(J::Bool(true))
        }
        b'f' if b[*i..].starts_with(b"false") => {
            *i += 5;
            Some(J::Bool(false))
        }
        b'n' if b[*i..].starts_with(b"null") => {
            *i += 4;
            Some(J::Null)
        }
        _ => {
            let s = *i;
            while *i < b.len() && (b[*i].is_ascii_digit() || b"+-.eE".contains(&b[*i])) {
                *i += 1;
            }
            std::str::from_utf8(&b[s..*i]).ok()?.parse().ok().map(J::Num)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_api_shapes() {
        let j = J::parse(r#"{"data":[{"text":"flying cars é 🚀","author_id":"1"}],"includes":{"users":[{"id":"1","username":"a"}]},"n":12,"ok":true,"z":null}"#).unwrap();
        assert_eq!(j.get("data").and_then(|d| d.at(0)).and_then(|p| p.get("text")).and_then(J::str), Some("flying cars é 🚀"));
        assert_eq!(j.get("n").and_then(J::num), Some(12.0));
        assert!(J::parse("{\"a\":}").is_none());
        let tl = J::parse(r#"{"data":[{"id":"7","author_id":"1","text":"low","public_metrics":{"like_count":3}},{"id":"9","author_id":"1","text":"top\npost","public_metrics":{"like_count":2400}}],"includes":{"users":[{"id":"1","username":"a"}]}}"#).unwrap();
        let top = x_top(&tl);
        assert_eq!(top[0], ("@a · ♥ 2.4K".to_string(), "top post".to_string(), "https://x.com/a/status/9".to_string()));
        let tr = x_trends(&J::parse(r#"{"data":[{"trend_name":"Rust 2.0","category":"Technology","post_count":"12K"}]}"#).unwrap());
        assert_eq!(tr[0].0, "Technology · 12K posts");
        assert!(tr[0].2.starts_with("https://x.com/search?q=Rust"));
        assert_eq!(badge_count("\"StatusLabel\"={ \"label\"=\"12\" }"), 12);
        assert_eq!(badge_count("\"StatusLabel\"={ \"label\"=\"•\" }"), 1);
        assert_eq!(badge_count("\"StatusLabel\"=[ NULL ]"), 0);
        let inv = stripe_rows(&J::parse(r#"{"data":[{"created":1700000000,"customer_email":"a@b.co","amount_paid":2900,"amount_due":2900,"status":"paid","number":"INV-1","hosted_invoice_url":"https://invoice.stripe.com/i/x","lines":{"data":[{"description":"Pro plan"}]}}]}"#).unwrap());
        assert_eq!((inv[0].who.as_str(), inv[0].what.as_str(), inv[0].amount, inv[0].url.as_str()), ("a@b.co", "Pro plan", 29.0, "https://invoice.stripe.com/i/x"));
        assert_eq!(env("plain"), "plain");
    }
}
