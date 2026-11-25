//! Media-player parts: rolling titles, cover art, transport, seek rail, volume, spectrum, karaoke lyrics.

use crate::image::{self, Image, Opts};
use crate::media::clock;
use crate::{fg, theme, Buffer, Frame, Rect, Style};

const SCROLL: f64 = 6.0; // cells per second
const HOLD: f64 = 1.5; // seconds parked at the start of each cycle
const EIGHTHS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Prints `text` as-is if it fits in `w`, else rolls it Spotify-style. Returns the x after it.
// shortcut: every char counts as one cell; wide glyphs will drift, upgrade with a width table.
pub fn marquee(b: &mut Buffer, x: u16, y: u16, w: u16, text: &str, time: f64, style: Style) -> u16 {
    if text.chars().count() <= w as usize {
        return b.print(x, y, text, style, w);
    }
    let cyc: Vec<char> = text.chars().chain("   ·   ".chars()).collect();
    let t = time.rem_euclid(cyc.len() as f64 / SCROLL + HOLD);
    let off = if t < HOLD { 0 } else { ((t - HOLD) * SCROLL) as usize };
    for i in 0..w as usize {
        b.set(x + i as u16, y, cyc[(off + i) % cyc.len()], style);
    }
    x + w
}

/// Cover art: the image if any, else a plain surface tile with a ♪. Returns the rect used.
pub fn cover(b: &mut Buffer, r: Rect, img: Option<&Image>, time: f64) -> Rect {
    if let Some(img) = img {
        return image::draw(b, r, img, Opts::default());
    }
    let w = r.w.min(r.h * 2);
    let h = w / 2;
    if h == 0 {
        return Rect::new(r.x, r.y, 0, 0);
    }
    let t = Rect::new(r.x + (r.w - w) / 2, r.y + (r.h - h) / 2, w, h);
    let _ = time;
    b.fill(t, theme::raised());
    let (cx, cy) = (t.x + w / 2, t.y + h / 2);
    b.set(cx, cy, '♪', Style::new().fg(theme::muted()).bg(theme::raised()));
    t
}

/// Hit ids for `transport`.
#[derive(Clone, Copy)]
pub struct Transport {
    pub prev: u32,
    pub play: u32,
    pub next: u32,
}

/// ⏮ ▶/⏸ ⏭ at (x, y); hover brightens, play sits in an accent pill. Returns the x after it.
pub fn transport(f: &mut Frame, x: u16, y: u16, playing: bool, ids: Transport) -> u16 {
    let mut cx = x;
    for (g, id) in [("⏮", ids.prev), ("", ids.play), ("⏭", ids.next)] {
        let r = Rect::new(cx, y, 3, 1);
        if id == ids.play {
            let bg = theme::accent().mix(theme::bg(), if f.hover(r) { 0.55 } else { 0.75 });
            f.buf.fill(r, bg);
            f.buf.set(cx + 1, y, if playing { '⏸' } else { '▶' }, Style::new().fg(theme::accent()).bg(bg).bold());
        } else {
            f.buf.set(cx + 1, y, g.chars().next().unwrap_or(' '), fg(if f.hover(r) { theme::text() } else { theme::muted() }));
        }
        f.hit(r, id);
        cx += 4;
    }
    cx - 1
}

/// Seek bar with an accent to blue fill. Hover shows a knob and the time under the cursor above it.
pub fn rail(f: &mut Frame, r: Rect, progress: f32, len: Option<f64>, id: u32) {
    if r.w == 0 {
        return;
    }
    let hot = f.hover(r);
    let k = (progress.clamp(0.0, 1.0) * (r.w - 1) as f32).round() as u16;
    for i in 0..r.w {
        let (ch, c) = if i == k && hot {
            ('●', theme::text())
        } else if i <= k {
            ('━', theme::accent().mix(theme::blue(), i as f32 / r.w.max(2) as f32))
        } else {
            ('─', theme::faint())
        };
        f.buf.set(r.x + i, r.y, ch, if i == k && hot { fg(c).bold() } else { fg(c) });
    }
    f.hit(r, id);
    if let (true, Some(len), Some((mx, _)), true) = (hot, len, f.mouse, r.y > 0) {
        let tip = format!(" {} ", clock(rail_time(mx - r.x, r, len)));
        let w = tip.chars().count() as u16;
        let x = mx.saturating_sub(w / 2).clamp(f.area.x, f.area.right().saturating_sub(w).max(f.area.x));
        f.buf.print(x, r.y - 1, &tip, Style::new().fg(theme::text()).bg(theme::raised()), w);
    }
}

/// Seconds at relative x in a rail of `len` seconds (same mapping a click uses to seek).
pub fn rail_time(x: u16, r: Rect, len: f64) -> f64 {
    x as f64 / r.w.max(1) as f64 * len
}

/// ◂ and four bars filled up to `v` (0..=1). Map a click with `volume_value`.
pub fn volume(f: &mut Frame, r: Rect, v: f32, id: u32) {
    if r.w < 2 {
        return;
    }
    let hot = f.hover(r);
    f.buf.set(r.x, r.y, '◂', fg(if hot { theme::text() } else { theme::muted() }));
    let n = r.w - 1;
    for i in 0..n {
        let on = (i + 1) as f32 / n as f32 <= v + 1e-3;
        let g = ['▁', '▃', '▅', '▇'][(i as usize * 4 / n as usize).min(3)];
        f.buf.set(r.x + 1 + i, r.y, g, fg(if on { theme::accent() } else { theme::faint() }));
    }
    f.hit(r, id);
}

/// Volume for a click at column `x` on a `volume` rect: the ◂ cell mutes, the last bar is full.
pub fn volume_value(x: u16, r: Rect) -> f32 {
    let n = r.w.saturating_sub(1).max(1);
    (x.saturating_sub(r.x) as f32 / n as f32).clamp(0.0, 1.0)
}

/// Spectrum bars with falling peak caps; feed it `media::spectrum` output each tick.
#[derive(Default, Clone)]
pub struct Spectrum {
    pub bands: Vec<f32>,
    pub peaks: Vec<f32>,
    /// Running loudness ceiling (auto-gain, like cava's autosens): quiet sources still fill the bars.
    pub gain: f32,
}

impl Spectrum {
    /// Fast attack, slow release: bars jump to a hit and settle, caps hang then drop.
    pub fn update(&mut self, fresh: &[f32], dt: f32) {
        if self.bands.len() != fresh.len() {
            (self.bands, self.peaks) = (vec![0.0; fresh.len()], vec![0.0; fresh.len()]);
        }
        let top = fresh.iter().fold(0.0f32, |a, &v| a.max(v));
        self.gain = if top > self.gain { top } else { (self.gain - dt * 0.1).max(top) };
        // floor keeps near-silence flat instead of amplifying noise
        let k = 0.9 / self.gain.max(0.15);
        for ((b, p), &v) in self.bands.iter_mut().zip(&mut self.peaks).zip(fresh) {
            let v = (v * k).min(1.0);
            *b = if v > *b { v } else { (*b - dt * 1.4).max(v) };
            *p = if *b > *p { *b } else { (*p - dt * 0.35).max(0.0) };
        }
    }

    /// One row: eighths bars. Taller: 2-col bars, peak caps, and (h >= 6) a dim reflection.
    pub fn draw(&self, b: &mut Buffer, r: Rect) {
        // One ink for every band: colour carries level, not position.
        let hue = |_: usize, _: usize| theme::accent();
        if r.h == 0 {
            return;
        }
        if r.h == 1 {
            let n = self.bands.len().min(r.w as usize);
            for i in 0..n {
                let v = self.bands[i * self.bands.len() / n];
                b.set(r.x + i as u16, r.y, EIGHTHS[((v * 8.0) as usize).clamp(1, 8)], fg(hue(i, n)));
            }
            return;
        }
        let n = self.bands.len().min(r.w as usize / 3);
        let rows = r.h as f32;
        let base = r.bottom();
        let pad = r.x + (r.w - (n as u16 * 3).saturating_sub(1)) / 2;
        for i in 0..n {
            // Stride across every band so a narrow rect still spans lows to highs.
            let j = i * self.bands.len() / n;
            let (v, p) = (self.bands[j], self.peaks[j]);
            let bx = pad + i as u16 * 3;
            let hue = hue(i, n);
            let h = v * rows;
            for k in 0..rows as u16 {
                let fill = (h - k as f32).clamp(0.0, 1.0);
                if fill <= 0.0 {
                    break;
                }
                let ch = if fill >= 1.0 { '█' } else { EIGHTHS[(fill * 8.0) as usize] };
                let c = hue.mix(theme::bg(), 0.45 * (1.0 - v));
                for dx in 0..2 {
                    b.set(bx + dx, base - 1 - k, ch, fg(c));
                }
            }
            let cap = (p * rows) as u16;
            if p > 0.08 && cap < rows as u16 && (p - v) * rows > 0.6 {
                for dx in 0..2 {
                    b.set(bx + dx, base - 1 - cap, '▔', fg(theme::muted()));
                }
            }
        }
    }
}

/// Small pill: accent-tinted background, bold text. Returns the x after it.
pub fn badge(b: &mut Buffer, x: u16, y: u16, label: &str) -> u16 {
    let st = Style::new().fg(theme::text()).bg(theme::accent().mix(theme::bg(), 0.7)).bold();
    b.set(x, y, ' ', st);
    let e = b.print(x + 1, y, label, st, u16::MAX - x - 2);
    b.set(e, y, ' ', st);
    e + 1
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Word {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    pub start: f64,
    pub end: f64,
    pub words: Vec<Word>,
}

impl Line {
    pub fn text(&self) -> String {
        self.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
    }
}

/// The line containing `now`, else the last one started.
pub fn current(lines: &[Line], now: f64) -> Option<usize> {
    lines.iter().position(|l| l.start <= now && now < l.end).or_else(|| lines.iter().rposition(|l| l.start <= now))
}

/// Greedy wrap of a line's words into rows of word indices.
fn rows(l: &Line, w: usize) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = vec![];
    let mut used = 0;
    for (i, wd) in l.words.iter().enumerate() {
        let n = wd.text.chars().count();
        match out.last_mut() {
            Some(row) if used + 1 + n <= w => {
                row.push(i);
                used += 1 + n;
            }
            _ => {
                out.push(vec![i]);
                used = n;
            }
        }
    }
    out
}

/// Karaoke view: current line centred at ~40% height with a per-word, per-char wipe; others fade by distance.
/// `scroll` is the eased line position in rows; keep it across frames.
pub fn lyrics(b: &mut Buffer, r: Rect, lines: &[Line], now: f64, scroll: &mut f32, dt: f32) {
    if r.w == 0 || r.h == 0 || lines.is_empty() {
        return;
    }
    let cur = current(lines, now);
    let wrapped: Vec<_> = lines.iter().map(|l| rows(l, r.w as usize)).collect();
    let mut top = Vec::with_capacity(lines.len());
    let mut y = 0.0f32;
    for w in &wrapped {
        top.push(y);
        y += w.len() as f32 + 1.0;
    }
    let ci = cur.unwrap_or(0);
    let target = top[ci] + wrapped[ci].len() as f32 / 2.0;
    *scroll += (target - *scroll) * (1.0 - (-12.0 * dt).exp());
    let anchor = r.y as f32 + r.h as f32 * 0.4;
    for (i, (l, rws)) in lines.iter().zip(&wrapped).enumerate() {
        let on = cur == Some(i);
        let dim = if i.abs_diff(ci) <= 1 { theme::muted() } else { theme::faint() };
        for (k, row) in rws.iter().enumerate() {
            let yy = (anchor + top[i] + k as f32 - *scroll).round();
            if yy < r.y as f32 || yy >= r.bottom() as f32 {
                continue;
            }
            let width: usize = row.iter().map(|&j| l.words[j].text.chars().count()).sum::<usize>() + row.len() - 1;
            let mut x = r.x + (r.w - (width as u16).min(r.w)) / 2;
            for &j in row {
                let wd = &l.words[j];
                let n = wd.text.chars().count();
                let sung = if !on {
                    0
                } else if now >= wd.end {
                    n
                } else if now > wd.start {
                    ((now - wd.start) / (wd.end - wd.start).max(1e-6) * n as f64).round() as usize
                } else {
                    0
                };
                for (c, ch) in wd.text.chars().enumerate() {
                    if x >= r.right() {
                        break;
                    }
                    let st = if !on {
                        fg(dim)
                    } else if c < sung {
                        fg(theme::accent()).bold()
                    } else {
                        fg(theme::text()).bold()
                    };
                    b.set(x, yy as u16, ch, st);
                    x += 1;
                }
                x += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(b: &Buffer, w: u16) -> String {
        (0..w).map(|x| b.get(x, 0).ch).collect()
    }

    #[test]
    fn marquee_fits_then_rolls() {
        let mut b = Buffer::new(8, 1);
        assert_eq!(marquee(&mut b, 0, 0, 8, "short", 3.0, Style::new()), 5);
        assert_eq!(text(&b, 5), "short");
        marquee(&mut b, 0, 0, 8, "a long song title", 0.0, Style::new());
        assert_eq!(text(&b, 8), "a long s");
        marquee(&mut b, 0, 0, 8, "a long song title", HOLD + 1.0, Style::new()); // 6 cells in
        assert_eq!(text(&b, 8), " song ti");
    }

    #[test]
    fn spectrum_auto_gain_lifts_quiet_keeps_silence() {
        let mut s = Spectrum::default();
        s.update(&[0.2, 0.1], 0.03); // quiet source fills most of the height
        assert!(s.bands[0] > 0.85 && s.bands[1] > 0.4);
        let mut s = Spectrum::default();
        s.update(&[0.01, 0.0], 0.03); // near-silence stays near flat
        assert!(s.bands[0] < 0.1);
    }

    #[test]
    fn rail_time_maps_x() {
        let r = Rect::new(5, 0, 10, 1);
        assert_eq!(rail_time(0, r, 100.0), 0.0);
        assert_eq!(rail_time(5, r, 100.0), 50.0);
    }

    #[test]
    fn lyrics_picks_current_line() {
        let l = |s: f64, e: f64| Line { start: s, end: e, words: vec![Word { start: s, end: e, text: "x".into() }] };
        let ls = [l(1.0, 3.0), l(3.0, 5.0), l(8.0, 9.0)];
        assert_eq!(current(&ls, 0.5), None);
        assert_eq!(current(&ls, 2.0), Some(0));
        assert_eq!(current(&ls, 3.0), Some(1));
        assert_eq!(current(&ls, 6.0), Some(1)); // gap: last started
        let (mut b, mut s) = (Buffer::new(10, 10), 0.0);
        lyrics(&mut b, Rect::new(0, 0, 10, 10), &ls, 2.0, &mut s, 1.0);
        assert_eq!(b.get(4, 4).ch, 'x'); // 40% of 10 rows
    }
}
