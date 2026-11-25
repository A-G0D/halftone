//! Media in the workspace: a few sources behind one card, opened on first use, each with its
//! cover, a cover-derived tint, optional background video and lyrics (LRC file or live whisper).

use super::config::{expand, Media};
use crate::deck::{Line, Word};
use crate::image::Image;
use crate::media::{Playback, Transcriber, Video};
use crate::{theme, Color};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

pub struct Item {
    pub label: String,
    pub src: String,
    pub bg: Option<String>,
    pub video: bool,
    pub pb: Option<Playback>,
    pub cover: Option<Image>,
    pub tint: Color,
    pub bgv: Option<Video>,
    pub bg_frame: Option<Image>,
    pub lrc: Vec<Line>,
    pub scribe: Option<Transcriber>,
    pub err: Option<String>,
    from: f64,
    /// Opening runs off the UI thread: links go through yt-dlp, which takes seconds.
    loading: Option<Receiver<Result<Playback, String>>>,
}

pub struct Deck {
    pub items: Vec<Item>,
    pub active: usize,
    covers: Receiver<(usize, Option<Image>)>,
}

/// Frame size asked of ffmpeg: enough for a full-screen pane in cells, sharper when the
/// terminal shows real pixels.
pub fn video_size() -> (u32, u32) {
    if crate::image::PIXELS.load(std::sync::atomic::Ordering::Relaxed) { (512, 288) } else { (224, 126) }
}

fn is_video(s: &str) -> bool {
    let l = s.to_lowercase();
    [".mp4", ".webm", ".mkv", ".mov"].iter().any(|e| l.ends_with(e)) || l.starts_with("http") || l.starts_with("ytsearch")
}

/// Average colour of an image, softened toward the theme so a loud cover only tints.
pub fn tint_of(img: &Image) -> Color {
    let n = img.px.len().max(1) as u32;
    let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
    for p in &img.px {
        (r, g, b) = (r + p[0] as u32, g + p[1] as u32, b + p[2] as u32);
    }
    Color::Rgb((r / n) as u8, (g / n) as u8, (b / n) as u8).mix(theme::muted(), 0.3)
}

/// `[mm:ss.xx] words` lines; each line ends where the next starts, words share its time evenly.
pub fn parse_lrc(text: &str) -> Vec<Line> {
    let mut out: Vec<(f64, String)> = vec![];
    for l in text.lines() {
        let Some(rest) = l.strip_prefix('[') else { continue };
        let Some((ts, words)) = rest.split_once(']') else { continue };
        let Some((m, s)) = ts.split_once(':') else { continue };
        if let (Ok(m), Ok(s)) = (m.parse::<f64>(), s.parse::<f64>()) {
            out.push((m * 60.0 + s, words.trim().to_string()));
        }
    }
    (0..out.len()).filter(|&i| !out[i].1.is_empty()).map(|i| {
        let start = out[i].0;
        let end = out.get(i + 1).map_or(start + 4.0, |n| n.0);
        let ws: Vec<&str> = out[i].1.split_whitespace().collect();
        let step = (end - start) * 0.85 / ws.len().max(1) as f64;
        let words = ws.iter().enumerate().map(|(k, w)| Word { start: start + k as f64 * step, end: start + (k + 1) as f64 * step, text: w.to_string() }).collect();
        Line { start, end, words }
    }).collect()
}

impl Deck {
    pub fn new(list: &[Media]) -> Deck {
        let (tx, covers) = mpsc::channel();
        let items = list.iter().enumerate().map(|(i, m)| {
            let src = expand(&m.src);
            let video = is_video(&src);
            if !src.starts_with("http") && !src.starts_with("ytsearch") {
                let (tx, s) = (tx.clone(), src.clone());
                thread::spawn(move || _ = tx.send((i, grab(&s, if video { 6.0 } else { 0.0 }))));
            }
            let lrc = std::fs::read_to_string(std::path::Path::new(&src).with_extension("lrc")).map(|t| parse_lrc(&t)).unwrap_or_default();
            Item { label: m.label.clone(), src, bg: m.bg.as_deref().map(expand), video, pb: None, cover: None, tint: theme::panel(), bgv: None, bg_frame: None, lrc, scribe: None, err: None, from: m.from, loading: None }
        }).collect();
        Deck { items, active: 0, covers }
    }

    pub fn loading(&self) -> bool {
        self.items.get(self.active).is_some_and(|i| i.loading.is_some())
    }

    pub fn item(&self) -> Option<&Item> {
        self.items.get(self.active)
    }

    pub fn pb(&mut self) -> Option<&mut Playback> {
        self.items.get_mut(self.active)?.pb.as_mut()
    }

    /// Switches source, pausing the one before; opens on first use.
    pub fn open(&mut self, i: usize) {
        if i >= self.items.len() {
            return;
        }
        if let Some(pb) = self.pb().filter(|p| p.playing) {
            let _ = pb.toggle();
        }
        self.active = i;
        let it = &mut self.items[i];
        match &mut it.pb {
            Some(pb) if !pb.playing => _ = pb.toggle(),
            Some(_) => {}
            None if it.loading.is_none() => {
                let (tx, rx) = mpsc::channel();
                let (src, size, from) = (it.src.clone(), it.video.then_some(video_size()), it.from);
                thread::spawn(move || {
                    let r = Playback::open(&src, size, true).and_then(|mut p| if from > 0.0 { p.seek_to(from).map(|_| p) } else { Ok(p) });
                    _ = tx.send(r);
                });
                it.loading = Some(rx);
            }
            None => {}
        }
    }

    pub fn tick(&mut self, dt: f64) {
        while let Ok((i, c)) = self.covers.try_recv() {
            if let Some(img) = &c {
                self.items[i].tint = tint_of(img);
            }
            self.items[i].cover = c;
        }
        let active = self.active;
        for (i, it) in self.items.iter_mut().enumerate() {
            if let Some(Ok(r)) = it.loading.as_ref().map(|rx| rx.try_recv()) {
                it.loading = None;
                match r {
                    Ok(mut p) => {
                        if i != active && p.playing {
                            let _ = p.toggle();
                        }
                        it.pb = Some(p);
                    }
                    Err(e) => it.err = Some(e),
                }
            }
            if let Some(pb) = &mut it.pb {
                pb.tick(dt);
                if it.cover.is_none() && it.video {
                    if let Some(f) = &pb.frame {
                        it.tint = tint_of(f);
                        it.cover = Some(f.clone());
                    }
                }
            }
        }
        let it = match self.items.get_mut(self.active) {
            Some(it) => it,
            None => return,
        };
        // Background video loops on its own clock, silent.
        if let Some(bg) = &it.bg {
            if it.bgv.as_mut().map_or(true, |v| v.ended()) {
                let (w, h) = video_size();
                it.bgv = Video::open(bg, w, h, 0.0).ok();
            }
            if let Some(f) = it.bgv.as_mut().and_then(|v| v.frame()) {
                it.bg_frame = Some(f);
            }
        }
    }

    /// Lines to show: the LRC file if there is one, else whatever whisper has heard so far.
    pub fn lines(&mut self, listen: bool) -> Vec<Line> {
        let Some(it) = self.items.get_mut(self.active) else { return vec![] };
        if !it.lrc.is_empty() {
            return it.lrc.clone();
        }
        if listen && it.scribe.is_none() {
            if let Some(pb) = &it.pb {
                it.scribe = Transcriber::start(&pb.src.audio, pb.pos).ok();
            }
        }
        it.scribe.as_ref().map(|s| s.lines.lock().map(|l| l.clone()).unwrap_or_default()).unwrap_or_default()
    }
}

/// The first frame ffmpeg gives from `at` (embedded art included), waiting up to 4 s.
pub fn grab(src: &str, at: f64) -> Option<Image> {
    let mut v = Video::open(src, 256, 256, at).ok().or_else(|| Video::open(src, 256, 256, 0.0).ok())?;
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

/// The cover as a spinning record: label in the middle turning at `angle`, grooves around it.
pub fn disc(cover: Option<&Image>, size: u32, angle: f32, bg: Color) -> Image {
    let (br, bgc, bb) = bg.rgb_or_black();
    let vinyl = theme::bg().mix(theme::text(), 0.06).rgb_or_black();
    let c = size as f32 / 2.0;
    let (sin, cos) = angle.sin_cos();
    let mut px = Vec::with_capacity((size * size) as usize);
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let d = (dx * dx + dy * dy).sqrt() / c;
            let p = if d > 1.0 || d < 0.05 {
                [br, bgc, bb]
            } else if d > 0.62 {
                // Grooves: faint rings with a sheen that turns with the record.
                let ring = ((d * 90.0).sin() * 0.5 + 0.5) * 6.0;
                let sheen = ((dy.atan2(dx) - angle) * 2.0).cos().max(0.0) * 10.0;
                let v = |c: u8| (c as f32 + ring + sheen).min(255.0) as u8;
                [v(vinyl.0), v(vinyl.1), v(vinyl.2)]
            } else if let Some(img) = cover {
                let (u, v) = (dx * cos + dy * sin, -dx * sin + dy * cos);
                let s = 0.62 * c;
                let ix = (((u / s) * 0.5 + 0.5) * img.w as f32).clamp(0.0, img.w as f32 - 1.0) as u32;
                let iy = (((v / s) * 0.5 + 0.5) * img.h as f32).clamp(0.0, img.h as f32 - 1.0) as u32;
                img.px[(iy * img.w + ix) as usize]
            } else {
                let t = theme::accent().rgb_or_black();
                [t.0, t.1, t.2]
            };
            px.push(p);
        }
    }
    Image { w: size, h: size, px }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lrc_lines_end_at_the_next() {
        let l = parse_lrc("[00:01.00] flying cars\n[00:03.50] are real\n");
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].start, l[0].end), (1.0, 3.5));
        assert_eq!(l[1].words[1].text, "real");
    }
}
