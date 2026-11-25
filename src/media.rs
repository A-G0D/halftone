//! Video, audio, spectrum and live captions for terminal apps, by driving `ffmpeg`, `ffplay`,
//! `yt-dlp` and `whisper-cli` as child processes. No codecs are linked; the same code runs on
//! macOS and Linux as long as those tools are on PATH.

use crate::deck::{Line, Word};
use crate::image::Image;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread;

fn spawn(tool: &str, args: &[String]) -> Result<Child, String> {
    Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| missing(tool, e))
}

fn missing(tool: &str, e: std::io::Error) -> String {
    let pkg = match tool {
        "yt-dlp" => "yt-dlp",
        "whisper-cli" => "whisper-cpp",
        _ => "ffmpeg",
    };
    format!("can't run {tool} ({e}). Install it: `brew install {pkg}` on macOS, your package manager on Linux.")
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// Playback speed from `HALFTONE_SPEED` (0.05 to 1.0, e.g. `0.25` for slow-motion recording), else 1.0.
pub fn speed() -> f64 {
    static SPEED: OnceLock<f64> = OnceLock::new();
    *SPEED.get_or_init(|| parse_speed(std::env::var("HALFTONE_SPEED").ok().as_deref()))
}

fn parse_speed(v: Option<&str>) -> f64 {
    v.and_then(|v| v.trim().parse::<f64>().ok()).filter(|f| f.is_finite() && *f > 0.0).map_or(1.0, |f| f.clamp(0.05, 1.0))
}

/// ffmpeg input arguments for a file, URL, or `mic` (the default microphone).
fn input(src: &str, start: f64) -> Vec<String> {
    if src == "mic" {
        return if cfg!(target_os = "macos") { s(&["-f", "avfoundation", "-i", ":0"]) } else { s(&["-f", "pulse", "-i", "default"]) };
    }
    // -readrate paces reading at the media's speed times HALFTONE_SPEED, so the reader thread is the clock.
    s(&["-readrate", &speed().to_string(), "-ss", &format!("{start:.2}"), "-i", src])
}

/// What to play: a title, the artist or channel if known, and the URLs or paths to read video and audio from.
#[derive(Clone, Debug)]
pub struct Source {
    pub title: String,
    pub artist: Option<String>,
    pub video: String,
    pub audio: String,
}

/// Resolves a local path as is, and a web page (YouTube and anything else yt-dlp knows) to its
/// stream URLs.
pub fn resolve(src: &str) -> Result<Source, String> {
    if !(src.starts_with("http://") || src.starts_with("https://") || src.starts_with("ytsearch")) {
        let stem = std::path::Path::new(src).file_stem().map_or(src.into(), |t| t.to_string_lossy().into_owned());
        let tag = |k: &str| {
            let out = Command::new("ffprobe").args(["-v", "error", "-show_entries", &format!("format_tags={k}"), "-of", "default=nw=1:nk=1", src]).output().ok()?;
            Some(String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|t| !t.is_empty())
        };
        let title = tag("title").unwrap_or(stem);
        return Ok(Source { title, artist: tag("artist"), video: src.into(), audio: src.into() });
    }
    let out = Command::new("yt-dlp")
        .args(["--no-warnings", "-f", "bv*[height<=480][vcodec^=avc1]+ba/b", "--print", "title", "--print", "uploader", "--print", "urls", src])
        .output()
        .map_err(|e| missing("yt-dlp", e))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let title = lines.next().unwrap_or(src).to_string();
    let artist = lines.next().filter(|u| *u != "NA").map(String::from);
    let video = lines.next().ok_or("yt-dlp returned no stream URL")?.to_string();
    let audio = lines.next().unwrap_or(&video).to_string();
    Ok(Source { title, artist, video, audio })
}

/// Length in seconds, if ffprobe can tell.
pub fn duration(src: &str) -> Option<f64> {
    let out = Command::new("ffprobe").args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", src]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Decoded frames, scaled and letterboxed to `w`×`h`, arriving in real time.
pub struct Video {
    child: Child,
    latest: Arc<Mutex<Option<Image>>>,
}

impl Video {
    pub fn open(src: &str, w: u32, h: u32, start: f64) -> Result<Video, String> {
        let (w, h) = (w.max(2) & !1, h.max(2) & !1);
        let mut args = input(src, start);
        args.extend(s(&[
            "-loglevel", "quiet", "-an",
            "-vf", &format!("scale={w}:{h}:force_original_aspect_ratio=decrease:force_divisible_by=2,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2"),
            "-f", "rawvideo", "-pix_fmt", "rgb24", "-",
        ]));
        let mut child = spawn("ffmpeg", &args)?;
        let mut out = child.stdout.take().ok_or("ffmpeg has no stdout")?;
        let latest = Arc::new(Mutex::new(None));
        let slot = latest.clone();
        thread::spawn(move || {
            let mut raw = vec![0u8; (w * h * 3) as usize];
            while out.read_exact(&mut raw).is_ok() {
                let px = raw.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
                *slot.lock().unwrap() = Some(Image { w, h, px });
            }
        });
        Ok(Video { child, latest })
    }

    /// The newest frame since the last call, if any.
    pub fn frame(&self) -> Option<Image> {
        self.latest.lock().unwrap().take()
    }

    pub fn ended(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Sound out of the speakers through ffplay. Set `HALFTONE_MUTE=1` to skip it (recordings, CI).
pub struct Audio(Option<Child>);

impl Audio {
    pub fn play(src: &str, start: f64, volume: u8) -> Result<Audio, String> {
        if std::env::var_os("HALFTONE_MUTE").is_some() {
            return Ok(Audio(None));
        }
        let args = s(&["-nodisp", "-autoexit", "-loglevel", "quiet", "-volume", &volume.min(100).to_string(), "-ss", &format!("{start:.2}"), src]);
        spawn("ffplay", &args).map(|c| Audio(Some(c)))
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        if let Some(c) = &mut self.0 {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// A live tap of mono samples, for spectrum bars and waveforms.
pub struct Pcm {
    child: Child,
    ring: Arc<Mutex<Vec<f32>>>,
}

impl Pcm {
    pub const RATE: u32 = 22050;
    const KEEP: usize = 8192;

    pub fn open(src: &str, start: f64) -> Result<Pcm, String> {
        let mut args = input(src, start);
        args.extend(s(&["-loglevel", "quiet", "-vn", "-f", "f32le", "-ac", "1", "-ar", &Pcm::RATE.to_string(), "-"]));
        let mut child = spawn("ffmpeg", &args)?;
        let mut out = child.stdout.take().ok_or("ffmpeg has no stdout")?;
        let ring = Arc::new(Mutex::new(Vec::with_capacity(Pcm::KEEP * 2)));
        let sink = ring.clone();
        thread::spawn(move || {
            let mut raw = [0u8; 2048];
            while out.read_exact(&mut raw).is_ok() {
                let mut r = sink.lock().unwrap();
                r.extend(raw.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])));
                let over = r.len().saturating_sub(Pcm::KEEP);
                r.drain(..over);
            }
        });
        Ok(Pcm { child, ring })
    }

    /// The last `n` samples (fewer at the start).
    pub fn latest(&self, n: usize) -> Vec<f32> {
        let r = self.ring.lock().unwrap();
        r[r.len().saturating_sub(n)..].to_vec()
    }
}

impl Drop for Pcm {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// In-place radix-2 FFT. `re.len()` must be a power of two.
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    assert!(n.is_power_of_two() && im.len() == n);
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let a = -std::f32::consts::TAU / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (wr, wi) = ((a * k as f32).cos(), (a * k as f32).sin());
                let (u, v) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[v] * wr - im[v] * wi, re[v] * wi + im[v] * wr);
                (re[v], im[v]) = (re[u] - tr, im[u] - ti);
                (re[u], im[u]) = (re[u] + tr, im[u] + ti);
            }
        }
        len <<= 1;
    }
}

/// `bands` log-spaced levels in 0..1 (a 60 dB window) from the newest power-of-two run of `samples`.
pub fn spectrum(samples: &[f32], bands: usize) -> Vec<f32> {
    let n = if samples.len() < 64 { return vec![0.0; bands] } else { 1 << samples.len().ilog2() };
    let tail = &samples[samples.len() - n..];
    let mut re: Vec<f32> = tail.iter().enumerate().map(|(i, x)| x * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos())).collect();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im);
    let half = n / 2;
    let mag = |k: usize| (re[k] * re[k] + im[k] * im[k]).sqrt() * 4.0 / n as f32;
    // Log-spaced bands from 40 Hz to near Nyquist, tilted +3 dB/octave so treble reads like it sounds.
    let bin = Pcm::RATE as f32 / n as f32;
    let (f0, f1) = (40.0f32, Pcm::RATE as f32 * 0.45);
    let edge = |b: usize| f0 * (f1 / f0).powf(b as f32 / bands as f32);
    (0..bands)
        .map(|b| {
            let lo = ((edge(b) / bin) as usize).max(1);
            let hi = ((edge(b + 1) / bin) as usize).max(lo + 1).min(half);
            let peak = (lo..hi).map(mag).fold(0.0, f32::max);
            let tilt = 3.0 * (edge(b) / 1000.0).log2();
            ((20.0 * peak.max(1e-6).log10() + tilt + 60.0) / 60.0).clamp(0.0, 1.0)
        })
        .collect()
}

/// The whisper.cpp model: `HALFTONE_WHISPER_MODEL`, or `~/.cache/halftone/ggml-base.en.bin`.
pub fn whisper_model() -> PathBuf {
    std::env::var_os("HALFTONE_WHISPER_MODEL")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache/halftone/ggml-base.en.bin"))
}

/// Live captions: audio is cut into chunks and each one is run through whisper-cli as it arrives.
pub struct Transcriber {
    child: Child,
    /// One timed-word line per chunk, in order.
    pub lines: Arc<Mutex<Vec<Line>>>,
}

impl Transcriber {
    // shortcut: fixed 5 s chunks can split a word at the seam; switch to whisper-stream's VAD if that shows.
    const CHUNK: f64 = 5.0;

    pub fn start(src: &str, start: f64) -> Result<Transcriber, String> {
        let model = whisper_model();
        if !model.exists() {
            return Err(format!(
                "no whisper model at {}. Download ggml-base.en.bin from https://huggingface.co/ggerganov/whisper.cpp or set HALFTONE_WHISPER_MODEL.",
                model.display()
            ));
        }
        let mut args = input(src, start);
        args.extend(s(&["-loglevel", "quiet", "-vn", "-f", "s16le", "-ac", "1", "-ar", "16000", "-"]));
        let mut child = spawn("ffmpeg", &args)?;
        let mut out = child.stdout.take().ok_or("ffmpeg has no stdout")?;
        let lines = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel::<(f64, Vec<u8>)>();

        // Reader: keeps the pipe drained so ffmpeg's pacing never stalls behind whisper.
        thread::spawn(move || {
            let mut t = start;
            loop {
                let mut chunk = Vec::new();
                let got = (&mut out).take((16000.0 * 2.0 * Self::CHUNK) as u64).read_to_end(&mut chunk).unwrap_or(0);
                if got >= 3200 && tx.send((t, chunk)).is_err() || got < 32000 * Self::CHUNK as usize {
                    break;
                }
                t += Self::CHUNK;
            }
        });

        let sink = lines.clone();
        thread::spawn(move || {
            let wav = std::env::temp_dir().join(format!("halftone-{}.wav", std::process::id()));
            for (t, pcm) in rx {
                if write_wav(&wav, &pcm).is_err() {
                    break;
                }
                let Ok(o) = Command::new("whisper-cli").arg("-m").arg(&model).arg("-f").arg(&wav).args(["-np", "-ml", "1", "-sow"]).stderr(Stdio::null()).output() else { break };
                let words = parse_words(&String::from_utf8_lossy(&o.stdout), t);
                if let (Some(a), Some(z)) = (words.first(), words.last()) {
                    let line = Line { start: a.start, end: z.end, words: words.clone() };
                    sink.lock().unwrap().push(line);
                }
            }
            let _ = std::fs::remove_file(wav);
        });
        Ok(Transcriber { child, lines })
    }
}

/// Parses `whisper-cli -ml 1 -sow` stdout (`[hh:mm:ss.mmm --> hh:mm:ss.mmm]  word`) into words offset by `t`.
/// A token without a leading space continues the previous word; `[..]` and `(..)` tokens are noise.
fn parse_words(out: &str, t: f64) -> Vec<Word> {
    let secs = |s: &str| {
        let p: Vec<f64> = s.trim().split(':').filter_map(|x| x.parse().ok()).collect();
        (p.len() == 3).then(|| p[0] * 3600.0 + p[1] * 60.0 + p[2])
    };
    let mut words: Vec<Word> = vec![];
    let mut noise = false; // inside a "(crowd cheering)" that whisper split into tokens
    for l in out.lines() {
        let Some((ts, rest)) = l.strip_prefix('[').and_then(|l| l.split_once(']')) else { continue };
        let Some((a, z)) = ts.split_once("-->") else { continue };
        let (Some(a), Some(z)) = (secs(a), secs(z)) else { continue };
        let tok = rest.strip_prefix("  ").unwrap_or(rest);
        let word = tok.trim();
        if word.starts_with('[') || word.starts_with('(') {
            noise = true;
        }
        if noise || word.is_empty() {
            noise &= !(word.ends_with(']') || word.ends_with(')'));
            continue;
        }
        match words.last_mut() {
            Some(w) if !tok.starts_with(' ') => {
                w.text += word;
                w.end = t + z;
            }
            _ => words.push(Word { start: t + a, end: t + z, text: word.into() }),
        }
    }
    words
}

impl Drop for Transcriber {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn write_wav(path: &std::path::Path, pcm: &[u8]) -> std::io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    let n = pcm.len() as u32;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + n).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    // PCM, mono, 16 kHz, 32000 bytes/s, block 2, 16 bits.
    for v in [16u32.to_le_bytes(), [1, 0, 1, 0], 16000u32.to_le_bytes(), 32000u32.to_le_bytes(), [2, 0, 16, 0]] {
        f.write_all(&v)?;
    }
    f.write_all(b"data")?;
    f.write_all(&n.to_le_bytes())?;
    f.write_all(pcm)
}

/// A clock plus whichever streams an app wants, restarted together on seek or pause.
pub struct Playback {
    pub src: Source,
    pub pos: f64,
    pub len: Option<f64>,
    pub playing: bool,
    pub volume: u8,
    /// The newest decoded frame, kept until a newer one arrives.
    pub frame: Option<Image>,
    size: Option<(u32, u32)>,
    pcm_on: bool,
    video: Option<Video>,
    audio: Option<Audio>,
    pcm: Option<Pcm>,
}

impl Playback {
    /// `size` asks for video frames of that many pixels; `pcm` asks for a sample tap.
    pub fn open(src: &str, size: Option<(u32, u32)>, pcm: bool) -> Result<Playback, String> {
        let src = resolve(src)?;
        let len = duration(&src.audio);
        let mut p = Playback { src, pos: 0.0, len, playing: true, volume: 80, frame: None, size, pcm_on: pcm, video: None, audio: None, pcm: None };
        p.restart()?;
        Ok(p)
    }

    fn restart(&mut self) -> Result<(), String> {
        (self.video, self.audio, self.pcm) = (None, None, None);
        if !self.playing {
            return Ok(());
        }
        if let Some((w, h)) = self.size {
            self.video = Some(Video::open(&self.src.video, w, h, self.pos)?);
        }
        self.audio = Some(Audio::play(&self.src.audio, self.pos, self.volume)?);
        if self.pcm_on {
            self.pcm = Some(Pcm::open(&self.src.audio, self.pos)?);
        }
        Ok(())
    }

    pub fn toggle(&mut self) -> Result<(), String> {
        self.playing = !self.playing;
        self.restart()
    }

    pub fn seek_to(&mut self, t: f64) -> Result<(), String> {
        self.pos = t.clamp(0.0, self.len.map_or(f64::MAX, |l| (l - 1.0).max(0.0)));
        self.restart()
    }

    pub fn seek(&mut self, dt: f64) -> Result<(), String> {
        self.seek_to(self.pos + dt)
    }

    pub fn set_volume(&mut self, v: i32) -> Result<(), String> {
        self.volume = v.clamp(0, 100) as u8;
        self.restart()
    }

    /// Changes the frame size; restarts video only when it actually changed.
    pub fn resize(&mut self, w: u32, h: u32) -> Result<(), String> {
        if self.size.is_some() && self.size != Some((w, h)) {
            self.size = Some((w, h));
            return self.restart();
        }
        Ok(())
    }

    /// Advances the clock and picks up the newest frame.
    // shortcut: audio and video run as separate processes started together, so they can drift
    // by a frame or two over long files; seek (or pause/play) re-aligns them.
    pub fn tick(&mut self, dt: f64) {
        if !self.playing {
            return;
        }
        self.pos += dt;
        if let Some(f) = self.video.as_ref().and_then(Video::frame) {
            self.frame = Some(f);
        }
        let ended = self.len.is_some_and(|l| self.pos >= l) || self.video.as_mut().is_some_and(Video::ended) && self.frame.is_some();
        if ended {
            self.playing = false;
            (self.video, self.audio, self.pcm) = (None, None, None);
        }
    }

    pub fn progress(&self) -> f32 {
        self.len.map_or(0.0, |l| (self.pos / l.max(0.001)).clamp(0.0, 1.0) as f32)
    }

    pub fn samples(&self, n: usize) -> Vec<f32> {
        self.pcm.as_ref().map_or_else(Vec::new, |p| p.latest(n))
    }
}

/// `m:ss`, or `h:mm:ss` past an hour.
pub fn clock(t: f64) -> String {
    let t = t.max(0.0) as u64;
    if t >= 3600 {
        format!("{}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
    } else {
        format!("{}:{:02}", t / 60, t % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_peaks_at_its_bin() {
        let n = 256;
        let mut re: Vec<f32> = (0..n).map(|i| (std::f32::consts::TAU * 19.0 * i as f32 / n as f32).sin()).collect();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im);
        let peak = (0..n / 2).max_by(|&a, &b| (re[a].hypot(im[a])).total_cmp(&re[b].hypot(im[b]))).unwrap();
        assert_eq!(peak, 19);
    }

    #[test]
    fn spectrum_is_bounded_and_silence_is_flat() {
        assert!(spectrum(&[0.0; 1024], 32).iter().all(|&v| v == 0.0));
        let tone: Vec<f32> = (0..2048).map(|i| (i as f32 * 0.3).sin()).collect();
        let s = spectrum(&tone, 32);
        assert_eq!(s.len(), 32);
        assert!(s.iter().all(|v| (0.0..=1.0).contains(v)) && s.iter().any(|&v| v > 0.8));
    }

    #[test]
    fn whisper_words() {
        let out = "\n[00:00:00.000 --> 00:00:00.050]  \n[00:00:00.050 --> 00:00:00.400]   Hello\n[00:00:00.400 --> 00:00:00.960]   world\n[00:00:00.960 --> 00:00:01.000]  ,\n[00:00:01.000 --> 00:00:01.500]   [MUSIC]\n[00:00:01.500 --> 00:00:02.000]   it's\n";
        let w = parse_words(out, 10.0);
        assert_eq!(w.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["Hello", "world,", "it's"]);
        assert_eq!((w[0].start, w[1].end, w[2].start), (10.05, 11.0, 11.5));
        let split = "[00:00:00.000 --> 00:00:00.300]   (crowd\n[00:00:00.300 --> 00:00:00.600]   cheering)\n[00:00:00.600 --> 00:00:01.000]   Whoa\n";
        assert_eq!(parse_words(split, 0.0).iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["Whoa"]);
    }

    #[test]
    fn clocks() {
        assert_eq!(clock(75.4), "1:15");
        assert_eq!(clock(3725.0), "1:02:05");
    }

    #[test]
    fn speed_parses_and_clamps() {
        assert_eq!(parse_speed(None), 1.0);
        assert_eq!(parse_speed(Some("0.25")), 0.25);
        assert_eq!(parse_speed(Some("0.001")), 0.05);
        assert_eq!(parse_speed(Some("9")), 1.0);
        assert_eq!(parse_speed(Some("0")), 1.0);
        assert_eq!(parse_speed(Some("nan")), 1.0);
    }
}
