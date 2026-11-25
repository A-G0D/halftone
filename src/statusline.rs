//! `halftone statusline`: Claude Code statusLine JSON on stdin -> one island-styled ANSI row. Includes a tiny JSON parser.

use halftone::island::pill;
use halftone::{term, Buffer, Color, Rect, Style};
use std::process::Command;

#[derive(Debug, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    /// Dotted path through objects, e.g. `cost.total_cost_usd`.
    pub fn at(&self, path: &str) -> Option<&J> {
        path.split('.').try_fold(self, |j, k| match j {
            J::Obj(m) => m.iter().find(|(n, _)| n == k).map(|(_, v)| v),
            _ => None,
        })
    }
    pub fn num(&self, path: &str) -> Option<f64> {
        match self.at(path)? {
            J::Num(n) => Some(*n),
            _ => None,
        }
    }
    pub fn str(&self, path: &str) -> Option<&str> {
        match self.at(path)? {
            J::Str(s) => Some(s),
            _ => None,
        }
    }
}

struct P<'a> {
    s: &'a [u8],
    i: usize,
}

pub fn parse(s: &str) -> Option<J> {
    let mut p = P { s: s.as_bytes(), i: 0 };
    let v = p.val(0)?;
    p.ws();
    (p.i == p.s.len()).then_some(v)
}

impl P<'_> {
    fn ws(&mut self) {
        while self.s.get(self.i).is_some_and(u8::is_ascii_whitespace) {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> Option<()> {
        self.ws();
        (self.s.get(self.i) == Some(&c)).then(|| self.i += 1)
    }
    fn lit(&mut self, w: &str, v: J) -> Option<J> {
        let ok = self.s[self.i..].starts_with(w.as_bytes());
        ok.then(|| {
            self.i += w.len();
            v
        })
    }
    fn val(&mut self, depth: u32) -> Option<J> {
        if depth > 64 {
            return None;
        }
        self.ws();
        match *self.s.get(self.i)? {
            b'{' => {
                self.i += 1;
                let mut m = vec![];
                if self.eat(b'}').is_some() {
                    return Some(J::Obj(m));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    self.eat(b':')?;
                    m.push((k, self.val(depth + 1)?));
                    if self.eat(b',').is_none() {
                        self.eat(b'}')?;
                        return Some(J::Obj(m));
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut a = vec![];
                if self.eat(b']').is_some() {
                    return Some(J::Arr(a));
                }
                loop {
                    a.push(self.val(depth + 1)?);
                    if self.eat(b',').is_none() {
                        self.eat(b']')?;
                        return Some(J::Arr(a));
                    }
                }
            }
            b'"' => self.string().map(J::Str),
            b't' => self.lit("true", J::Bool(true)),
            b'f' => self.lit("false", J::Bool(false)),
            b'n' => self.lit("null", J::Null),
            _ => {
                let st = self.i;
                while self.s.get(self.i).is_some_and(|c| matches!(c, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')) {
                    self.i += 1;
                }
                std::str::from_utf8(&self.s[st..self.i]).ok()?.parse().ok().map(J::Num)
            }
        }
    }
    fn hex4(&mut self) -> Option<u32> {
        let h = std::str::from_utf8(self.s.get(self.i..self.i + 4)?).ok()?;
        self.i += 4;
        u32::from_str_radix(h, 16).ok()
    }
    fn string(&mut self) -> Option<String> {
        if self.s.get(self.i) != Some(&b'"') {
            return None;
        }
        self.i += 1;
        let mut out = Vec::new();
        loop {
            let c = *self.s.get(self.i)?;
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let e = *self.s.get(self.i)?;
                    self.i += 1;
                    let ch = match e {
                        b'n' => '\n',
                        b't' => '\t',
                        b'r' => '\r',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'u' => {
                            let mut u = self.hex4()?;
                            if (0xd800..0xdc00).contains(&u) && self.s[self.i..].starts_with(b"\\u") {
                                self.i += 2;
                                let lo = self.hex4()?;
                                u = 0x10000 + ((u - 0xd800) << 10) + lo.wrapping_sub(0xdc00);
                            }
                            char::from_u32(u).unwrap_or('\u{fffd}')
                        }
                        c => c as char, // " \ /
                    };
                    out.extend_from_slice(ch.to_string().as_bytes());
                }
                c => out.push(c),
            }
        }
    }
}

const DEMO: &str = r#"{"model":{"id":"claude-opus-4","display_name":"Opus 4"},"workspace":{"current_dir":"."},
"context_window":{"used_percentage":62.4},"cost":{"total_cost_usd":1.2345,"total_lines_added":156,"total_lines_removed":23},
"rate_limits":{"five_hour":{"used_percentage":42}}}"#;

const GREY: Color = Color::rgb(0x636366);
const TRACK: Color = Color::rgb(0x2c2c2e);
const LEFT: [char; 9] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

enum Piece {
    T(String, Color),
    Bar(f32, Color),
}

/// Green below 50%, yellow to 80%, red past it.
fn heat(pct: f64) -> Color {
    let (g, y, r) = (Color::rgb(0x30d158), Color::rgb(0xffd60a), Color::rgb(0xff453a));
    if pct < 50.0 { g.mix(y, pct as f32 / 50.0) } else { y.mix(r, ((pct - 50.0) / 30.0) as f32) }
}

fn branch(dir: &str) -> Option<String> {
    let o = Command::new("git").args(["-C", dir, "branch", "--show-current"]).stderr(std::process::Stdio::null()).output().ok()?;
    let b = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (o.status.success() && !b.is_empty()).then_some(b)
}

fn render(j: &J, width: u16) -> String {
    let t = |s: String, c: Color| Piece::T(s, c);
    let white = Color::rgb(0xf2f2f7);
    let mut segs: Vec<Vec<Piece>> = vec![];
    if let Some(m) = j.str("model.display_name") {
        segs.push(vec![t(m.into(), white)]);
    }
    if let Some(p) = j.num("context_window.used_percentage") {
        segs.push(vec![Piece::Bar((p / 100.0) as f32, heat(p)), t(format!(" {p:.0}%"), heat(p))]);
    }
    if let Some(c) = j.num("cost.total_cost_usd") {
        segs.push(vec![t(format!("${c:.2}"), Color::rgb(0xaeaeb2))]);
    }
    if let Some(p) = j.num("rate_limits.five_hour.used_percentage") {
        segs.push(vec![t(format!("5h {p:.0}%"), heat(p))]);
    }
    if let Some(b) = j.str("workspace.current_dir").and_then(branch) {
        segs.push(vec![t(format!("⎇ {b}"), Color::rgb(0x64d2ff))]);
    }
    let (add, del) = (j.num("cost.total_lines_added"), j.num("cost.total_lines_removed"));
    if add.is_some() || del.is_some() {
        segs.push(vec![t(format!("+{:.0}", add.unwrap_or(0.0)), Color::rgb(0x30d158)), t(" ".into(), white), t(format!("−{:.0}", del.unwrap_or(0.0)), Color::rgb(0xff453a))]);
    }
    if segs.is_empty() {
        segs.push(vec![t("claude".into(), white)]);
    }
    let wide = |p: &Piece| match p {
        Piece::T(s, _) => s.chars().count(),
        Piece::Bar(..) => 8,
    };
    let content: usize = segs.iter().flatten().map(wide).sum::<usize>() + 3 * (segs.len() - 1);
    let w = (content + 4).min(width.max(8) as usize) as u16;
    let mut b = Buffer::new(w, 1);
    pill(&mut b, Rect::new(0, 0, w, 1));
    let (mut x, end) = (2u16, w - 2);
    for (i, seg) in segs.iter().enumerate() {
        if i > 0 {
            x = b.print(x, 0, " · ", Style::new().fg(GREY).bg(Color::Rgb(0, 0, 0)), end.saturating_sub(x));
        }
        for p in seg {
            match p {
                Piece::T(s, c) => x = b.print(x, 0, s, Style::new().fg(*c).bg(Color::Rgb(0, 0, 0)), end.saturating_sub(x)),
                Piece::Bar(v, c) => {
                    let eighths = (v.clamp(0.0, 1.0) * 64.0).round() as usize;
                    for k in 0..8 {
                        let e = eighths.saturating_sub(k * 8).min(8);
                        if x < end {
                            b.set(x, 0, LEFT[e], Style::new().fg(*c).bg(TRACK));
                            x += 1;
                        }
                    }
                }
            }
        }
    }
    term::to_ansi(&b, true).trim_end_matches('\n').to_string()
}

pub fn run(args: &[String]) {
    let mut input = String::new();
    if args.iter().any(|a| a == "--demo") {
        input = DEMO.into();
    } else {
        let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut input);
    }
    let width = std::env::var("COLUMNS").ok().and_then(|c| c.parse().ok()).unwrap_or(100);
    println!("{}", render(&parse(&input).unwrap_or(J::Null), width));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_escapes_and_rejects_junk() {
        let j = parse(r#"{"a":{"b":[1,-2.5e1,true,null,"x\"é😀"]},"s":"q\\n"}"#).unwrap();
        assert_eq!(j.str("s"), Some("q\\n"));
        assert!(matches!(j.at("a.b"), Some(J::Arr(v)) if v.len() == 5 && v[1] == J::Num(-25.0) && v[4] == J::Str("x\"é😀".into())));
        assert!(parse("{\"a\":").is_none() && parse("nope").is_none() && parse("{} x").is_none());
    }
}
