//! `halftone connect x`: signs in to X in the browser (OAuth 2.0 with PKCE, no client secret) and
//! keeps the tokens in ~/.config/halftone/x.token, readable only by you. The X card refreshes them.

use super::connect::J;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const PORT: u16 = 8723;
const SCOPES: &str = "tweet.read users.read offline.access";

pub fn redirect() -> String {
    format!("http://127.0.0.1:{PORT}/callback")
}

pub fn path() -> PathBuf {
    super::config::path().with_file_name("x.token")
}

/// What `connect x` keeps between runs.
#[derive(Debug, PartialEq)]
pub struct Tok {
    pub client: String,
    pub access: String,
    pub refresh: String,
    pub expires: u64,
    pub user: String,
    pub handle: String,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Tok {
    pub fn load() -> Option<Tok> {
        let t = std::fs::read_to_string(path()).ok()?;
        let v = |k: &str| t.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).unwrap_or("").to_string();
        Some(Tok { client: v("client"), access: v("access"), refresh: v("refresh"), expires: v("expires").parse().unwrap_or(0), user: v("user"), handle: v("handle") })
    }

    fn save(&self) -> Result<(), String> {
        let p = path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let body = format!("client={}\naccess={}\nrefresh={}\nexpires={}\nuser={}\nhandle={}\n", self.client, self.access, self.refresh, self.expires, self.user, self.handle);
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
        o.open(&p).and_then(|mut f| f.write_all(body.as_bytes())).map_err(|e| format!("{}: {e}", p.display()))
    }

    /// A usable access token, refreshing first when it is about to lapse (they last two hours).
    pub fn fresh() -> Result<Tok, String> {
        let mut t = Tok::load().ok_or("not signed in · run: halftone setup x")?;
        if t.expires > now() + 60 {
            return Ok(t);
        }
        let j = form("grant_type=refresh_token", &[("refresh_token", &t.refresh), ("client_id", &t.client)])?;
        t.access = j.get("access_token").and_then(J::str).ok_or("sign-in expired · run: halftone connect x")?.into();
        if let Some(r) = j.get("refresh_token").and_then(J::str) {
            t.refresh = r.into();
        }
        t.expires = now() + j.get("expires_in").and_then(J::num).unwrap_or(7200.0) as u64;
        t.save()?;
        Ok(t)
    }
}

/// POSTs a form to the token endpoint; the body (codes, tokens) goes on stdin, not the command line.
fn form(first: &str, rest: &[(&str, &str)]) -> Result<J, String> {
    let mut body = first.to_string();
    for (k, v) in rest {
        body += &format!("&{k}={}", enc(v));
    }
    let mut c = Command::new("curl")
        .args(["-sS", "--max-time", "20", "-X", "POST", "-H", "Content-Type: application/x-www-form-urlencoded", "--data-binary", "@-", "https://api.x.com/2/oauth2/token"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|_| "needs curl".to_string())?;
    c.stdin.take().ok_or("no stdin")?.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
    let o = c.wait_with_output().map_err(|e| e.to_string())?;
    let j = J::parse(&String::from_utf8_lossy(&o.stdout)).ok_or("X sent an unreadable reply")?;
    if let Some(e) = j.get("error_description").or(j.get("error")).and_then(J::str) {
        return Err(format!("X said: {e}"));
    }
    Ok(j)
}

pub fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn b64url(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..=c.len() {
            out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut m = data.to_vec();
    m.push(0x80);
    while m.len() % 64 != 56 {
        m.push(0);
    }
    m.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in m.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    let mut out = [0u8; 32];
    for i in 0..8 {
        out[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

fn random(n: usize) -> Result<Vec<u8>, String> {
    let mut b = vec![0u8; n];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).map_err(|e| format!("random: {e}"))?;
    Ok(b)
}

/// The whole sign-in: browser, callback on 127.0.0.1, code for tokens, then who you are.
pub fn connect(args: &[String]) -> Result<(), String> {
    let client = match args.first() {
        Some(c) => c.clone(),
        None => {
            println!("X sign-in for halftone. You need an X developer app first:");
            println!("  1. console.x.com: create a project and an app (type: Native App)");
            println!("  2. User authentication: Read, callback URL {}", redirect());
            println!("  3. Copy the OAuth 2.0 Client ID\n");
            print!("Client ID: ");
            std::io::stdout().flush().ok();
            let mut s = String::new();
            std::io::stdin().read_line(&mut s).map_err(|e| e.to_string())?;
            s.trim().to_string()
        }
    };
    if client.is_empty() {
        return Err("no client id".into());
    }
    let verifier = b64url(&random(32)?);
    let challenge = b64url(&sha256(verifier.as_bytes()));
    let state = b64url(&random(16)?);
    let url = format!(
        "https://x.com/i/oauth2/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}&state={state}&code_challenge={challenge}&code_challenge_method=S256",
        enc(&client), enc(&redirect()), enc(SCOPES)
    );
    let listener = TcpListener::bind(("127.0.0.1", PORT)).map_err(|e| format!("port {PORT} is busy: {e}"))?;
    println!("Opening X in your browser to approve read access. If it does not open:\n{url}\n");
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = Command::new(opener).arg(&url).stdout(Stdio::null()).stderr(Stdio::null()).spawn();

    let (code, got_state) = loop {
        let (mut s, _) = listener.accept().map_err(|e| e.to_string())?;
        let mut line = String::new();
        BufReader::new(&s).read_line(&mut line).map_err(|e| e.to_string())?;
        let q = line.split_whitespace().nth(1).and_then(|p| p.strip_prefix("/callback?")).unwrap_or("").to_string();
        let get = |k: &str| q.split('&').find_map(|kv| kv.strip_prefix(&format!("{k}="))).map(String::from);
        let page = |msg: &str| format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<body style=\"font:16px system-ui;padding:48px;background:#0b0c0e;color:#e7e9ea\">{msg}</body>");
        if let Some(e) = get("error") {
            let _ = s.write_all(page("X sign-in was cancelled. You can close this tab.").as_bytes());
            return Err(format!("X said: {e}"));
        }
        if let (Some(c), Some(st)) = (get("code"), get("state")) {
            let _ = s.write_all(page("Signed in. Back to your terminal; you can close this tab.").as_bytes());
            break (c, st);
        }
        let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n");
    };
    if got_state != state {
        return Err("sign-in state did not match; try again".into());
    }
    // The code lives 30 seconds, so trade it straight away.
    let j = form("grant_type=authorization_code", &[("code", &code), ("redirect_uri", &redirect()), ("code_verifier", &verifier), ("client_id", &client)])?;
    let access = j.get("access_token").and_then(J::str).ok_or("X sent no token")?.to_string();
    let mut t = Tok {
        client,
        refresh: j.get("refresh_token").and_then(J::str).unwrap_or("").into(),
        expires: now() + j.get("expires_in").and_then(J::num).unwrap_or(7200.0) as u64,
        access,
        user: String::new(),
        handle: String::new(),
    };
    let me = super::connect::get_json("https://api.x.com/2/users/me", &format!("Bearer {}", t.access))?;
    t.user = me.get("data").and_then(|d| d.get("id")).and_then(J::str).unwrap_or("").into();
    t.handle = me.get("data").and_then(|d| d.get("username")).and_then(J::str).unwrap_or("").into();
    t.save()?;
    // Turn the card on: `connect x @signin` reads this token file.
    let cfg = super::config::load()?;
    if !cfg.connects.iter().any(|(s, _)| s == "x") {
        let mut f = std::fs::OpenOptions::new().append(true).open(super::config::path()).map_err(|e| e.to_string())?;
        writeln!(f, "connect x @signin").map_err(|e| e.to_string())?;
    }
    println!("Signed in as @{}. Your X card shows your Following feed now.", t.handle);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_and_pkce() {
        let hex: String = sha256(b"abc").iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        // S256 challenge, checked against Python's hashlib + urlsafe_b64encode.
        assert_eq!(b64url(&sha256(b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gXFJrOTjM")), "EWm4BmC5oS6ms4Zzngt_Hg09CBE_TVDTPTx4yHHlaYU");
        assert_eq!((b64url(b"ab"), b64url(b"a")), ("YWI".to_string(), "YQ".to_string()));
        assert_eq!(enc("a b/c"), "a%20b%2Fc");
    }
}
