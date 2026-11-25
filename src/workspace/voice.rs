//! Voice mod: ctrl+space starts listening, ctrl+space again stops; whisper turns it into text that
//! is typed into the focused terminal (Claude Code, codex, a shell). Mic via ffmpeg; set
//! `HALFTONE_VOICE_FILE` to speak from a recording instead (demos, tests).

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct Voice {
    child: Child,
    pcm: Arc<Mutex<Vec<i16>>>,
}

const RATE: usize = 16_000;

impl Voice {
    pub fn start() -> Result<Voice, String> {
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-loglevel", "error"]);
        match std::env::var("HALFTONE_VOICE_FILE") {
            Ok(f) => cmd.args(["-re", "-i", &f]),
            Err(_) if cfg!(target_os = "macos") => cmd.args(["-f", "avfoundation", "-i", ":0"]),
            Err(_) => cmd.args(["-f", "pulse", "-i", "default"]),
        };
        let mut child = cmd
            .args(["-ac", "1", "-ar", "16000", "-f", "s16le", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("voice needs ffmpeg: {e}"))?;
        let mut out = child.stdout.take().ok_or("no ffmpeg output")?;
        let pcm = Arc::new(Mutex::new(Vec::new()));
        let sink = pcm.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 3200];
            while let Ok(n) = out.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let s: Vec<i16> = buf[..n & !1].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
                if let Ok(mut v) = sink.lock() {
                    v.extend(s);
                }
            }
        });
        Ok(Voice { child, pcm })
    }

    pub fn tick(&mut self) {}

    /// Loudness of the last 16 tenths of a second, 0..1, oldest first.
    pub fn levels(&self) -> Vec<f32> {
        let v = self.pcm.lock().map(|v| v.clone()).unwrap_or_default();
        let win = RATE / 10;
        let n = v.len() / win;
        (n.saturating_sub(16)..n).map(|i| {
            let s = &v[i * win..(i + 1) * win];
            let rms = (s.iter().map(|&x| (x as f32 / 32768.0).powi(2)).sum::<f32>() / win as f32).sqrt();
            (rms * 6.0).min(1.0)
        }).collect()
    }

    /// Stops listening and returns what whisper heard.
    pub fn finish(mut self) -> Result<String, String> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let pcm = self.pcm.lock().map(|v| v.clone()).unwrap_or_default();
        if pcm.len() < RATE / 2 {
            return Ok(String::new());
        }
        let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache/halftone");
        let _ = std::fs::create_dir_all(&dir);
        let wav = dir.join("voice.wav");
        std::fs::write(&wav, wav_bytes(&pcm)).map_err(|e| e.to_string())?;
        let model = crate::media::whisper_model();
        if !model.exists() {
            return Err(format!("voice needs a whisper model at {} (halftone mod add voice)", model.display()));
        }
        let o = Command::new("whisper-cli")
            .args(["-m", &model.to_string_lossy(), "-f", &wav.to_string_lossy(), "-nt", "-np"])
            .output()
            .map_err(|e| format!("voice needs whisper-cli: {e}"))?;
        let text = String::from_utf8_lossy(&o.stdout);
        Ok(text.split_whitespace().collect::<Vec<_>>().join(" "))
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// 16 kHz mono 16-bit PCM in a minimal RIFF/WAVE wrapper.
pub fn wav_bytes(pcm: &[i16]) -> Vec<u8> {
    let data = (pcm.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&(RATE as u32).to_le_bytes());
    b.extend_from_slice(&(RATE as u32 * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in pcm {
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}

#[cfg(test)]
mod tests {
    #[test]
    fn wav_header_sizes() {
        let b = super::wav_bytes(&[0, 1, -1]);
        assert_eq!(b.len(), 44 + 6);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(b[40..44].try_into().unwrap()), 6);
    }
}
