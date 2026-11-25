//! Your real coding-agent sessions, newest first: Claude Code transcripts in ~/.claude/projects,
//! Codex rollouts in ~/.codex/sessions and DeepSeek Harness logs in ~/.dsh/sessions. Opening one resumes it in the folder it ran in, which you
//! already trusted, so the agent asks no trust question.

use super::connect::J;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    pub tool: String,
    pub id: String,
    pub cwd: String,
    /// The chat's title (yours, else the one Claude gave it), else your last prompt.
    pub task: String,
    /// What it is doing now ("editing cards.rs"), or how its last turn ended.
    pub doing: String,
    /// Seconds since it last wrote anything.
    pub idle: f64,
}

impl Session {
    pub fn repo(&self) -> String {
        Path::new(&self.cwd).file_name().map_or(self.cwd.clone(), |n| n.to_string_lossy().into_owned())
    }

    /// The command that reopens it where it ran. A session busy elsewhere is forked so the two
    /// never write the same transcript.
    pub fn command(&self) -> String {
        let dir = format!("'{}'", self.cwd.replace('\'', "'\\''"));
        match self.tool.as_str() {
            "codex" => format!("cd {dir} && codex resume {}", self.id),
            "deepseek" => format!("cd {dir} && dsh tui --resume {}", self.id),
            _ if self.idle < 120.0 => format!("cd {dir} && claude --resume {} --fork-session", self.id),
            _ => format!("cd {dir} && claude --resume {}", self.id),
        }
    }
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

fn age(p: &Path) -> Option<f64> {
    let m = std::fs::metadata(p).ok()?.modified().ok()?;
    Some(SystemTime::now().duration_since(m).map_or(0.0, |d| d.as_secs_f64()))
}

/// The last `n` bytes as whole lines; transcripts can be large.
fn tail(p: &Path, n: u64) -> Vec<String> {
    let Ok(mut f) = std::fs::File::open(p) else { return vec![] };
    let len = f.metadata().map_or(0, |m| m.len());
    let _ = f.seek(SeekFrom::Start(len.saturating_sub(n)));
    let mut s = String::new();
    let _ = f.take(n).read_to_string(&mut s);
    let mut lines: Vec<String> = s.lines().map(String::from).collect();
    if len > n && !lines.is_empty() {
        lines.remove(0); // cut mid-line
    }
    lines
}

fn first_line(p: &Path) -> Option<String> {
    let mut s = String::new();
    std::fs::File::open(p).ok()?.take(64 * 1024).read_to_string(&mut s).ok();
    s.lines().next().map(String::from)
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A user turn's text, skipping tool results and the harness's own <tags>.
fn prompt_text(msg: &J) -> Option<String> {
    let c = msg.get("content")?;
    let text = match c {
        J::Str(s) => s.clone(),
        J::Arr(parts) => parts.iter().filter(|p| p.get("type").and_then(J::str) == Some("text")).filter_map(|p| p.get("text").and_then(J::str)).collect::<Vec<_>>().join(" "),
        _ => return None,
    };
    let t = one_line(&text);
    (!t.is_empty() && !t.starts_with('<')).then_some(t)
}

/// A folder that still exists and is not a temp dir, where background `claude -p` runs land.
fn kept(cwd: &str) -> bool {
    let tmp = std::env::temp_dir();
    let tmp = tmp.to_string_lossy();
    let temp = ["/tmp/", "/private/tmp/", "/private/var/folders/", "/var/folders/", tmp.as_ref()].iter().any(|t| format!("{cwd}/").starts_with(t));
    !temp && Path::new(cwd).is_dir()
}

fn base(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn short(s: &str, n: usize) -> String {
    let s = one_line(s);
    if s.chars().count() > n { format!("{}…", s.chars().take(n).collect::<String>()) } else { s }
}

/// A tool call as a few words: Edit of src/a.rs is "editing a.rs".
fn activity(name: &str, input: Option<&J>) -> String {
    let arg = |k: &str| input.and_then(|i| i.get(k)).and_then(J::str).unwrap_or("");
    match name {
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => format!("editing {}", base(arg("file_path"))),
        "Read" => format!("reading {}", base(arg("file_path"))),
        "Bash" if !arg("description").is_empty() => short(&arg("description").to_lowercase(), 48),
        "Bash" => format!("running {}", short(arg("command"), 40)),
        "Grep" | "Glob" => format!("searching {}", short(arg("pattern"), 30)),
        "WebSearch" | "WebFetch" | "web_search" | "web_fetch" => "reading the web".into(),
        "bash" => format!("running {}", short(arg("command"), 40)),
        "spawn_teammate" | "send_message" | "list_agents" => "working with its team".into(),
        "wait_agent" => "waiting on its team".into(),
        "Agent" | "Task" => "working with a subagent".into(),
        n if n.starts_with("mcp__") => format!("using {}", n.rsplit("__").next().unwrap_or(n)),
        n => n.to_lowercase().replace('_', " "),
    }
}

/// Title from either kind of title record.
fn title_of(j: &J) -> Option<String> {
    match j.get("type").and_then(J::str)? {
        "custom-title" => j.get("customTitle").and_then(J::str).map(one_line),
        "ai-title" => j.get("aiTitle").and_then(J::str).map(one_line),
        _ => None,
    }.filter(|t| !t.is_empty())
}

fn claude(p: &Path, idle: f64) -> Option<Session> {
    let (mut id, mut cwd, mut task) = (String::new(), String::new(), String::new());
    let (mut title, mut doing) = (None::<String>, String::new());
    let mut edited: Vec<String> = vec![];
    let lines = tail(p, 256 * 1024);
    for l in lines.iter().rev() {
        let Some(j) = J::parse(l) else { continue };
        if title.is_none() {
            title = title_of(&j);
        }
        // Tool calls from the newest back: the latest is what it is doing, edits since your last
        // prompt are what it changed.
        if task.is_empty() && j.get("type").and_then(J::str) == Some("assistant") {
            for c in j.get("message").and_then(|m| m.get("content")).and_then(J::arr).into_iter().flatten() {
                if c.get("type").and_then(J::str) != Some("tool_use") {
                    continue;
                }
                let name = c.get("name").and_then(J::str).unwrap_or("");
                if doing.is_empty() {
                    doing = activity(name, c.get("input"));
                }
                if matches!(name, "Edit" | "MultiEdit" | "Write" | "NotebookEdit") {
                    let f = c.get("input").and_then(|i| i.get("file_path")).and_then(J::str).unwrap_or("").to_string();
                    if !edited.contains(&f) {
                        edited.push(f);
                    }
                }
            }
        }
        let Some(j) = J::parse(l) else { continue };
        if cwd.is_empty() {
            if let (Some(c), Some(s)) = (j.get("cwd").and_then(J::str), j.get("sessionId").and_then(J::str)) {
                (cwd, id) = (c.into(), s.into());
            }
        }
        if task.is_empty() && j.get("type").and_then(J::str) == Some("last-prompt") {
            task = j.get("lastPrompt").and_then(J::str).map(one_line).unwrap_or_default();
        }
        if task.is_empty() && j.get("type").and_then(J::str) == Some("user") {
            if let Some(t) = j.get("message").and_then(prompt_text) {
                task = t;
            }
        }
        if !cwd.is_empty() && !task.is_empty() && title.is_some() {
            break;
        }
    }
    if title.is_none() {
        // Titles are written early; a long chat's tail may not hold one.
        let mut s = String::new();
        std::fs::File::open(p).ok()?.take(512 * 1024).read_to_string(&mut s).ok();
        title = s.lines().filter_map(J::parse).filter_map(|j| title_of(&j)).last();
    }
    if idle >= 60.0 {
        doing = match edited.len() {
            0 => "done".into(),
            1 => format!("done · {} changed", base(&edited[0])),
            n => format!("done · {n} files changed"),
        };
    }
    let task = title.unwrap_or(task);
    (!id.is_empty() && kept(&cwd)).then(|| Session { tool: "claude".into(), id, cwd, task, doing, idle })
}

fn codex(p: &Path, idle: f64) -> Option<Session> {
    let meta = J::parse(&first_line(p)?)?;
    let pl = meta.get("payload")?;
    let id = pl.get("id").and_then(J::str)?.to_string();
    let cwd = pl.get("cwd").and_then(J::str)?.to_string();
    let lines = tail(p, 256 * 1024);
    let payloads: Vec<J> = lines.iter().rev().filter_map(|l| J::parse(l)?.get("payload").cloned()).collect();
    let kind = |pl: &J| pl.get("type").and_then(J::str).unwrap_or("").to_string();
    let task = payloads.iter().find(|pl| kind(pl) == "user_message").and_then(|pl| pl.get("message").and_then(J::str).map(one_line)).unwrap_or_default();
    let doing = match payloads.iter().find(|pl| matches!(kind(pl).as_str(), "custom_tool_call" | "function_call" | "task_complete" | "user_message")) {
        _ if idle >= 60.0 => "done".into(),
        Some(pl) if kind(pl) == "task_complete" || kind(pl) == "user_message" => "done".into(),
        Some(pl) => {
            // Codex wraps shell commands as `tools.exec_command({cmd:"…"})`.
            let raw = pl.get("input").or(pl.get("arguments")).and_then(J::str).unwrap_or("");
            raw.split("cmd:\"").nth(1).or(raw.split("\"command\":[\"").nth(1)).map(|c| format!("running {}", short(c.split('"').next().unwrap_or(""), 40))).unwrap_or_else(|| "working".into())
        }
        None => String::new(),
    };
    kept(&cwd).then(|| Session { tool: "codex".into(), id, cwd, task, doing, idle })
}

/// A DeepSeek Harness session: zstd-compressed JSON lines, read through the `zstd` tool.
fn deepseek(p: &Path, idle: f64) -> Option<Session> {
    // Lead sessions live in `session-…` folders, subagents in bare ids: skip those unread.
    if !p.parent()?.file_name()?.to_string_lossy().starts_with("session-") {
        return None;
    }
    let o = std::process::Command::new("zstd").args(["-dcq", "--"]).arg(p).stderr(std::process::Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&o.stdout);
    let meta = J::parse(text.lines().next()?)?;
    // Teammates and subagents belong to a lead session; only top-level ones are listed.
    if meta.get("origin").and_then(J::str) == Some("subagent") {
        return None;
    }
    let id = meta.get("id").and_then(J::str)?.to_string();
    let cwd = meta.get("cwd").and_then(J::str)?.to_string();
    let (mut title, mut task, mut doing) = (String::new(), String::new(), String::new());
    for l in text.lines().rev() {
        let kind = l.get(9..40).unwrap_or("");
        if !(kind.starts_with("session/title\"") || kind.starts_with("user/message") || kind.starts_with("tool/call\"")) {
            continue;
        }
        let Some(j) = J::parse(l) else { continue };
        let d = j.get("data");
        match j.get("type").and_then(J::str).unwrap_or("") {
            "session/title" if title.is_empty() => title = d.and_then(|d| d.get("title")).and_then(J::str).map(one_line).unwrap_or_default(),
            "user/message" if task.is_empty() => task = d.and_then(|d| d.get("content")).and_then(J::arr).and_then(|c| c.iter().find_map(|p| p.get("text").and_then(J::str))).map(|t| short(t, 80)).unwrap_or_default(),
            "tool/call" if doing.is_empty() => {
                let name = d.and_then(|d| d.get("name")).and_then(J::str).unwrap_or("");
                let input = d.and_then(|d| d.get("arguments")).and_then(J::str).and_then(J::parse);
                doing = activity(name, input.as_ref());
            }
            _ => {}
        }
        if !title.is_empty() && !task.is_empty() && !doing.is_empty() {
            break;
        }
    }
    if idle >= 60.0 {
        doing = "done".into();
    }
    let task = if title.is_empty() { task } else { title };
    (!task.is_empty() && kept(&cwd)).then(|| Session { tool: "deepseek".into(), id, cwd, task, doing, idle })
}

/// Every `*.jsonl` (or `*.zstd`) under `dir`, up to `depth` folders down, with its age.
fn files(dir: &Path, depth: u32, out: &mut Vec<(f64, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() && depth > 0 {
            files(&p, depth - 1, out);
        } else if p.extension().is_some_and(|x| x == "jsonl" || x == "zstd") {
            if let Some(a) = age(&p) {
                out.push((a, p));
            }
        }
    }
}

/// The newest `n` sessions across both tools.
pub fn scan(n: usize) -> Vec<Session> {
    let h = home();
    let mut cl = vec![];
    files(&h.join(".claude/projects"), 1, &mut cl);
    let mut cx = vec![];
    files(&h.join(".codex/sessions"), 3, &mut cx);
    let mut ds = vec![];
    files(&h.join(".dsh/sessions"), 2, &mut ds);
    let tag = |v: Vec<(f64, PathBuf)>, k: u8| v.into_iter().map(move |(a, p)| (a, p, k));
    let mut all: Vec<(f64, PathBuf, u8)> = tag(cl, 0).chain(tag(cx, 1)).chain(tag(ds, 2)).collect();
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<Session> = vec![];
    for (a, p, kind) in all {
        if out.len() >= n {
            break;
        }
        let s = match kind { 0 => claude(&p, a), 1 => codex(&p, a), _ => deepseek(&p, a) };
        // A session nobody typed into yet has nothing to come back to.
        if let Some(s) = s.filter(|s| !s.task.is_empty() && !out.iter().any(|o| o.id == s.id)) {
            out.push(s);
        }
    }
    out
}

/// Rescans every fifteen seconds off the UI thread.
pub fn watch(n: usize) -> Receiver<Vec<Session>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || loop {
        if tx.send(scan(n)).is_err() {
            return;
        }
        thread::sleep(Duration::from_secs(15));
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_resume_in_place() {
        let s = |idle| Session { tool: "claude".into(), id: "abc".into(), cwd: "/tmp/it's".into(), task: String::new(), doing: String::new(), idle };
        assert_eq!(activity("Edit", J::parse(r#"{"file_path":"/a/src/cards.rs"}"#).as_ref()), "editing cards.rs");
        assert_eq!(activity("Bash", J::parse(r#"{"command":"cargo test","description":"Run the tests"}"#).as_ref()), "run the tests");
        assert_eq!(title_of(&J::parse(r#"{"type":"ai-title","aiTitle":"Arc reopen crash"}"#).unwrap()).as_deref(), Some("Arc reopen crash"));
        assert_eq!(s(600.0).command(), "cd '/tmp/it'\\''s' && claude --resume abc");
        assert!(s(5.0).command().ends_with("--fork-session"));
        assert_eq!(prompt_text(&J::parse(r#"{"content":[{"type":"text","text":"fix  the\ntests"}]}"#).unwrap()).as_deref(), Some("fix the tests"));
        assert!(prompt_text(&J::parse(r#"{"content":"<system-reminder>x"}"#).unwrap()).is_none());
        assert!(!kept("/private/var/folders/3d/x/T") && !kept("/tmp") && kept("/"));
    }
}
