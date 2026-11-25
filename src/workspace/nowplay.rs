//! What is playing elsewhere on this machine, read once a second: Spotify and Apple Music through
//! their AppleScript dictionaries on macOS, any MPRIS player through `playerctl` on Linux.
//! Halftone only watches and sends play/pause/next; the audio stays in the app that plays it.

use crate::image::Image;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub app: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub pos: f64,
    pub dur: f64,
    pub playing: bool,
    pub art: String,
}

pub struct Monitor {
    pub now: Option<Track>,
    /// Seconds since `now` arrived, so the rail moves smoothly between polls.
    pub age: f64,
    pub cover: Option<Image>,
    cover_for: String,
    rx: Receiver<Option<Track>>,
    covers: (Sender<(String, Option<Image>)>, Receiver<(String, Option<Image>)>),
}

/// Spotify reports duration in milliseconds, Music in seconds; both report position in seconds.
const MAC: &[(&str, &str, f64)] = &[
    ("Spotify", "/Applications/Spotify.app", 1000.0),
    ("Music", "/System/Applications/Music.app", 1.0),
];

fn script(app: &str) -> String {
    let art = if app == "Spotify" { "artwork url of current track" } else { "\"\"" };
    format!(
        "if application \"{app}\" is running then
  tell application \"{app}\"
    if player state is not stopped then
      return (player state as string) & tab & name of current track & tab & artist of current track & tab & album of current track & tab & (player position as string) & tab & (duration of current track as string) & tab & {art}
    end if
  end tell
end if
return \"\""
    )
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let o = Command::new(cmd).args(args).stderr(Stdio::null()).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// One reading: a playing source beats a paused one.
pub fn read() -> Option<Track> {
    let num = |s: &str| s.replace(',', ".").trim().parse::<f64>().unwrap_or(0.0);
    let mut found: Vec<Track> = vec![];
    if cfg!(target_os = "macos") {
        for (app, path, per_sec) in MAC {
            if !std::path::Path::new(path).exists() {
                continue;
            }
            let Some(out) = run("osascript", &["-e", &script(app)]) else { continue };
            let f: Vec<&str> = out.split('\t').collect();
            if f.len() >= 7 {
                found.push(Track {
                    app: app.to_string(), playing: f[0] == "playing", title: f[1].into(), artist: f[2].into(), album: f[3].into(),
                    pos: num(f[4]), dur: num(f[5]) / per_sec, art: f[6].into(),
                });
            }
        }
    } else if let Some(out) = run("playerctl", &["metadata", "--format", "{{playerName}}\t{{status}}\t{{title}}\t{{artist}}\t{{album}}\t{{position}}\t{{mpris:length}}\t{{mpris:artUrl}}"]) {
        let f: Vec<&str> = out.split('\t').collect();
        if f.len() >= 8 && f[1] != "Stopped" {
            // MPRIS times are microseconds.
            found.push(Track { app: f[0].into(), playing: f[1] == "Playing", title: f[2].into(), artist: f[3].into(), album: f[4].into(), pos: num(f[5]) / 1e6, dur: num(f[6]) / 1e6, art: f[7].into() });
        }
    }
    found.sort_by_key(|t| !t.playing);
    found.into_iter().next()
}

/// Sends play/pause, next or previous to whichever app the track came from.
pub fn control(app: &str, what: &str) {
    let mac = match what { "next" => "next track", "previous" => "previous track", _ => "playpause" };
    if cfg!(target_os = "macos") {
        let _ = Command::new("osascript").args(["-e", &format!("tell application \"{app}\" to {mac}")]).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    } else {
        let what = if what == "toggle" { "play-pause" } else { what };
        let _ = Command::new("playerctl").args(["-p", app, what]).spawn();
    }
}

impl Monitor {
    pub fn start(demo: Option<Track>) -> Monitor {
        let (tx, rx) = mpsc::channel();
        match demo {
            // Recordings show a fixed track instead of whatever this machine is playing.
            Some(t) => _ = tx.send(Some(t)),
            None => {
                thread::spawn(move || loop {
                    if tx.send(read()).is_err() {
                        return;
                    }
                    thread::sleep(Duration::from_millis(1000));
                });
            }
        }
        Monitor { now: None, age: 0.0, cover: None, cover_for: String::new(), rx, covers: mpsc::channel() }
    }

    pub fn tick(&mut self, dt: f64) {
        self.age += dt;
        while let Ok(t) = self.rx.try_recv() {
            self.now = t;
            self.age = 0.0;
        }
        while let Ok((key, img)) = self.covers.1.try_recv() {
            if key == self.cover_for {
                self.cover = img;
            }
        }
        let want = self.now.as_ref().map_or(String::new(), |t| if t.art.is_empty() { t.album.clone() } else { t.art.clone() });
        if want != self.cover_for {
            self.cover_for = want.clone();
            self.cover = None;
            if let Some(t) = &self.now {
                let src = t.art.clone();
                if src.starts_with("http") || src.starts_with('/') || src.starts_with("file://") {
                    let tx = self.covers.0.clone();
                    let src = src.trim_start_matches("file://").to_string();
                    thread::spawn(move || _ = tx.send((want, super::media::grab(&src, 0.0))));
                }
            }
        }
    }

    /// Position now, moving on from the last reading while playing.
    pub fn pos(&self) -> f64 {
        self.now.as_ref().map_or(0.0, |t| if t.playing { (t.pos + self.age).min(t.dur.max(t.pos)) } else { t.pos })
    }

    /// Jumps to `secs` in the playing track.
    pub fn seek(&mut self, secs: f64) {
        let Some(t) = &mut self.now else { return };
        let secs = secs.clamp(0.0, t.dur.max(0.0));
        if cfg!(target_os = "macos") {
            let _ = Command::new("osascript").args(["-e", &format!("tell application \"{}\" to set player position to {secs:.1}", t.app)]).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
        } else {
            let _ = Command::new("playerctl").args(["-p", &t.app, "position", &format!("{secs:.1}")]).spawn();
        }
        (t.pos, self.age) = (secs, 0.0);
    }

    pub fn control(&mut self, what: &str) {
        if let Some(t) = &mut self.now {
            control(&t.app, what);
            if what == "toggle" {
                // Flip at once; the next reading confirms it.
                t.pos = self.age.mul_add(if t.playing { 1.0 } else { 0.0 }, t.pos);
                self.age = 0.0;
                t.playing = !t.playing;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn scripts_ask_only_running_apps() {
        let s = super::script("Spotify");
        assert!(s.starts_with("if application \"Spotify\" is running") && s.contains("artwork url"));
        assert!(!super::script("Music").contains("artwork url"));
    }
}
