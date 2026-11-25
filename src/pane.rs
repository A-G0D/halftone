//! `halftone island`: an always-on island pane showing which coding agents are active, from local session files only.

use halftone::prelude::*;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const LIVE: f64 = 8.0; // seconds since last write that count as working
const RESCAN: f64 = 4.0;

extern "C" {
    fn localtime_r(t: *const i64, tm: *mut u8) -> *mut u8;
}

/// Local (h, m, s). struct tm starts with sec, min, hour as ints on every unix libc.
fn clock() -> (i32, i32, i32) {
    let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let mut tm = [0u8; 64];
    // SAFETY: tm is larger than any libc's struct tm, localtime_r only writes into it.
    unsafe { localtime_r(&t, tm.as_mut_ptr()) };
    let f = |i: usize| i32::from_ne_bytes(tm[i * 4..i * 4 + 4].try_into().unwrap());
    (f(2), f(1), f(0))
}

fn newest_in(dir: &Path) -> Option<(SystemTime, PathBuf)> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max()
}

/// Last `"name":"tool"` in the file's final 4 KB.
fn last_tool(p: &Path) -> Option<String> {
    let mut f = File::open(p).ok()?;
    let n = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(n.saturating_sub(4096))).ok()?;
    let mut b = vec![];
    f.read_to_end(&mut b).ok()?;
    let s = String::from_utf8_lossy(&b);
    let i = s.rfind("\"name\":\"")? + 8;
    let t: String = s[i..].chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect();
    (!t.is_empty()).then_some(t)
}

struct Watch {
    name: &'static str,
    icon: char,
    tint: u32,
    root: PathBuf,
    nested: bool, // codex: sessions/YYYY/MM/DD/*.jsonl, claude: projects/<proj>/*.jsonl
    file: Option<PathBuf>,
    scanned: f64,
}

impl Watch {
    /// Newest session file, rescanned every few seconds from only the freshest dirs (no full-tree walk).
    fn rescan(&mut self) {
        // key is mtime for project dirs, name for date dirs; keep the top `n`
        let latest = |d: &Path, by_time: bool, n: usize| {
            let mut v: Vec<_> = fs::read_dir(d).ok()?.flatten().filter(|e| e.path().is_dir()).map(|e| {
                let t = if by_time { e.metadata().and_then(|m| m.modified()).ok() } else { None };
                (t, e.file_name(), e.path())
            }).collect();
            v.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
            Some(v.into_iter().take(n).map(|x| x.2).collect::<Vec<_>>())
        };
        let dirs = if self.nested {
            (0..3).try_fold(vec![self.root.clone()], |ds, _| latest(&ds[0], false, 1)).unwrap_or_default()
        } else {
            latest(&self.root, true, 4).unwrap_or_default()
        };
        self.file = dirs.iter().filter_map(|d| newest_in(d)).max().map(|(_, p)| p);
    }

    fn poll(&mut self, t: f64) -> Option<Activity> {
        if !self.root.is_dir() {
            return None;
        }
        if t - self.scanned >= RESCAN || self.scanned == 0.0 {
            (self.scanned, ) = (t.max(0.001), );
            self.rescan();
        }
        let age = self.file.as_ref().and_then(|p| fs::metadata(p).ok()?.modified().ok()?.elapsed().ok()).unwrap_or(Duration::MAX).as_secs_f64();
        let live = age < LIVE;
        let tool = if live { self.file.as_deref().and_then(last_tool) } else { None };
        let mins = age / 60.0;
        let idle = if age == Duration::MAX.as_secs_f64() { "no sessions".into() } else if mins < 90.0 { format!("idle {:.0}m", mins.max(1.0)) } else { format!("idle {:.0}h", mins / 60.0) };
        let st = if live { "working" } else { "idle" };
        Some(Activity {
            icon: self.icon,
            tint: Color::rgb(if live { self.tint } else { 0x636366 }),
            lead: self.name.into(),
            trail: if live { "live".into() } else { "idle".into() },
            title: format!("{} · {st}", self.name),
            detail: if live { tool.map_or("working".into(), |t| format!("tool: {t}")) } else { idle },
            progress: None,
            live,
        })
    }
}

pub struct Pane {
    isl: Island,
    w: [Watch; 2],
    acts: Vec<Activity>,
    t: f64,
    dt: f32,
    polled: f64,
    rotated: f64,
}

impl Pane {
    pub fn new() -> Pane {
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
        let w = |name, icon, tint, sub: &str, nested| Watch { name, icon, tint, root: home.join(sub), nested, file: None, scanned: 0.0 };
        let mut isl = Island::default();
        isl.max_w = u16::MAX;
        Pane { isl, w: [w("claude", '✻', 0xd97757, ".claude/projects", false), w("codex", '◆', 0x10a37f, ".codex/sessions", true)], acts: vec![], t: 0.0, dt: 0.0, polled: -9.0, rotated: 0.0 }
    }

    fn poll(&mut self) {
        let mut a: Vec<Activity> = self.w.iter_mut().filter_map(|w| w.poll(self.t)).collect();
        a.sort_by_key(|x| !x.live);
        let (h, m, s) = clock();
        a.push(Activity { icon: '◷', tint: Color::rgb(0x0a84ff), trail: format!("{h:02}:{m:02}"), title: "clock".into(), detail: format!("{h:02}:{m:02}:{s:02}"), ..Default::default() });
        self.acts = a;
    }
}

impl App for Pane {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match *ev {
            Event::Tick(dt) => {
                (self.t, self.dt) = (self.t + dt, self.dt + dt as f32);
                if self.t - self.polled >= 1.0 {
                    (self.polled, ) = (self.t, );
                    self.poll();
                }
                if self.t - self.rotated >= 5.0 {
                    self.rotated = self.t;
                    // rotate through live agents only, or everything when none is live
                    let live = self.acts.iter().filter(|a| a.live).count();
                    let n = if live > 0 { live } else { self.acts.len() };
                    self.isl.active = (self.isl.active + 1) % n.max(1);
                }
            }
            Event::Click { id: 1, .. } => self.isl.toggle(),
            Event::Click { id: 2, .. } => self.isl.next(),
            _ => match (ev.key(), ev.ch()) {
                (Some(KeyCode::Tab), _) => self.isl.next(),
                (_, Some('q')) => ctx.quit(),
                _ => {}
            },
        }
    }

    fn view(&mut self, f: &mut Frame) {
        self.isl.expanded = f.area.h >= 4;
        let dt = std::mem::take(&mut self.dt);
        let a = std::mem::take(&mut self.acts);
        self.isl.draw(f, f.area, &a, dt, 1);
        self.acts = a;
    }
}
