//! Login for when AI Company is reachable from a network (e.g. a VPS behind nginx).
//!
//! Enabled only when both AUTH_USER and AUTH_PASSWORD_HASH are set (create the hash with
//! `ai-company hash-password`). On a local machine without them nothing changes.
//!
//! - Sessions: random 256-bit tokens in an HttpOnly + Secure + SameSite=Strict cookie,
//!   kept in memory only (not on disk, where agent-run code could read them). 7 days.
//! - Brute force: 5 failures per IP → locked 15 min; 30 failures overall → everyone locked 15 min.
//! - CSRF / cross-site WebSocket: every state-changing request and every WebSocket upgrade must
//!   carry an Origin equal to the page's own origin (a project preview on another origin cannot).
//! - Agents' own browser reaches previews directly on 127.0.0.1 (no proxy header) without login;
//!   everything else always needs a session.
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::Request;
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const COOKIE: &str = "aico_session";
const SESSION_MS: u64 = 7 * 24 * 3600 * 1000;
const LOCK_MS: u64 = 15 * 60 * 1000;
const MAX_FAILS_PER_IP: u32 = 5;
const MAX_FAILS_TOTAL: u32 = 30;

struct Auth {
    user: String,
    hash: String,
    secure_cookie: bool,
    /// token → expiry (ms)
    sessions: Mutex<HashMap<String, u64>>,
    /// ip → (failures, first failure ms)
    fails: Mutex<HashMap<String, (u32, u64)>>,
}

static AUTH: OnceLock<Option<Auth>> = OnceLock::new();

fn now() -> u64 {
    crate::store::now_ms()
}

/// Reads the configuration once at startup. Returns whether login is required.
pub fn init() -> bool {
    let a = AUTH.get_or_init(|| {
        let user = std::env::var("AUTH_USER").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())?;
        let hash = std::env::var("AUTH_PASSWORD_HASH").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())?;
        if PasswordHash::new(&hash).is_err() {
            eprintln!("AUTH_PASSWORD_HASH không hợp lệ: tạo lại bằng `ai-company hash-password`");
            std::process::exit(1);
        }
        let secure_cookie = std::env::var("AUTH_COOKIE_SECURE").map(|v| v != "0").unwrap_or(true);
        Some(Auth { user, hash, secure_cookie, sessions: Mutex::new(HashMap::new()), fails: Mutex::new(HashMap::new()) })
    });
    a.is_some()
}

fn auth() -> Option<&'static Auth> {
    AUTH.get().and_then(|a| a.as_ref())
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("OS random generator unavailable");
    b
}

pub fn hash_password(pw: &str) -> String {
    let salt = SaltString::encode_b64(&random_bytes::<16>()).expect("salt");
    Argon2::default().hash_password(pw.as_bytes(), &salt).expect("argon2").to_string()
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    // Constant time for equal lengths; the length itself is not secret enough to matter here.
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn check(a: &Auth, user: &str, pw: &str) -> bool {
    // Always run the slow hash so a wrong user name takes as long as a wrong password.
    let pw_ok = PasswordHash::new(&a.hash).map(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok()).unwrap_or(false);
    ct_eq(user.as_bytes(), a.user.as_bytes()) & pw_ok
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

fn session_ok(a: &Auth, headers: &HeaderMap) -> bool {
    let Some(tok) = cookie_value(headers, COOKIE) else { return false };
    let mut s = a.sessions.lock().unwrap();
    let t = now();
    s.retain(|_, exp| *exp > t);
    s.keys().any(|k| ct_eq(k.as_bytes(), tok.as_bytes()))
}

/// The client address as seen by nginx (X-Real-IP), or "direct" for local callers.
fn client_ip(headers: &HeaderMap) -> String {
    headers.get("x-real-ip").and_then(|v| v.to_str().ok()).map(str::to_string).unwrap_or_else(|| "direct".into())
}

/// True when the request carries an Origin equal to its own host (same-origin browser request),
/// or one listed in ALLOWED_ORIGINS (the UI on Cloudflare Pages, which proxies to us).
fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else { return false };
    if std::env::var("ALLOWED_ORIGINS").is_ok_and(|list| list.split(',').any(|o| o.trim().trim_end_matches('/') == origin)) {
        return true;
    }
    let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) else { return false };
    origin == format!("https://{host}") || origin == format!("http://{host}")
}

/// Marks a preview request served without a session (PREVIEW_PUBLIC=1): internal files are hidden.
#[derive(Clone, Copy)]
pub struct PublicPreview;

fn is_public(path: &str) -> bool {
    matches!(path, "/login" | "/api/auth/login" | "/api/auth/logout" | "/api/auth/status")
}

/// axum middleware guarding everything when login is enabled.
pub async fn guard(req: Request, next: Next) -> Response {
    let Some(a) = auth() else { return next.run(req).await };
    let path = req.uri().path().to_string();
    let headers = req.headers().clone();
    let headers = &headers;
    let via_proxy = headers.contains_key("x-real-ip");
    let is_ws = headers.get(header::UPGRADE).is_some();
    let mutating = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);

    // CSRF and cross-site WebSocket hijacking.
    if (mutating || is_ws) && via_proxy && !same_origin(headers) {
        return (StatusCode::FORBIDDEN, "Yêu cầu không cùng nguồn (Origin) bị chặn").into_response();
    }
    if is_public(&path) {
        return next.run(req).await;
    }
    // The employees' own headless browser opens previews on 127.0.0.1 directly.
    if !via_proxy && req.method() == Method::GET && path.starts_with("/preview/") {
        return next.run(req).await;
    }
    if session_ok(a, headers) {
        return next.run(req).await;
    }
    if req.method() == Method::GET && path.starts_with("/preview/") && std::env::var("PREVIEW_PUBLIC").is_ok_and(|v| v == "1") {
        let mut req = req;
        req.extensions_mut().insert(PublicPreview);
        return next.run(req).await;
    }
    if path.starts_with("/api/") || path == "/ws" {
        return (StatusCode::UNAUTHORIZED, "Chưa đăng nhập").into_response();
    }
    let target = if path.starts_with('/') && !path.starts_with("//") { path } else { "/".into() };
    Redirect::to(&format!("/login?next={}", urlencode(&target))).into_response()
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_./~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

#[derive(Deserialize)]
pub struct LoginBody {
    user: String,
    password: String,
}

pub async fn login(headers: HeaderMap, Json(b): Json<LoginBody>) -> Response {
    let Some(a) = auth() else { return (StatusCode::NOT_FOUND, "Không bật đăng nhập").into_response() };
    let ip = client_ip(&headers);
    let t = now();
    {
        let mut f = a.fails.lock().unwrap();
        f.retain(|_, (_, first)| t - *first < LOCK_MS);
        let total: u32 = f.values().map(|(n, _)| *n).sum();
        let mine = f.get(&ip).map(|(n, first)| (*n, *first));
        if total >= MAX_FAILS_TOTAL || mine.is_some_and(|(n, _)| n >= MAX_FAILS_PER_IP) {
            let first = mine.map(|(_, f)| f).unwrap_or(t);
            let mins = ((LOCK_MS.saturating_sub(t - first)) / 60_000).max(1);
            return (StatusCode::TOO_MANY_REQUESTS, format!("Sai quá nhiều lần. Thử lại sau khoảng {mins} phút.")).into_response();
        }
    }
    let (user, pw) = (b.user.chars().take(100).collect::<String>(), b.password.chars().take(200).collect::<String>());
    let ok = tokio::task::spawn_blocking(move || check(a, &user, &pw)).await.unwrap_or(false);
    if !ok {
        let mut f = a.fails.lock().unwrap();
        let e = f.entry(ip).or_insert((0, t));
        e.0 += 1;
        return (StatusCode::UNAUTHORIZED, "Sai tài khoản hoặc mật khẩu").into_response();
    }
    a.fails.lock().unwrap().remove(&ip);
    let token: String = random_bytes::<32>().iter().map(|b| format!("{b:02x}")).collect();
    a.sessions.lock().unwrap().insert(token.clone(), t + SESSION_MS);
    let cookie = format!(
        "{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}{}",
        SESSION_MS / 1000,
        if a.secure_cookie { "; Secure" } else { "" }
    );
    ([(header::SET_COOKIE, cookie)], Json(json!({ "ok": true }))).into_response()
}

pub async fn logout(headers: HeaderMap) -> Response {
    if let (Some(a), Some(tok)) = (auth(), cookie_value(&headers, COOKIE)) {
        a.sessions.lock().unwrap().remove(&tok);
    }
    let cookie = format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0");
    ([(header::SET_COOKIE, cookie)], Json(json!({ "ok": true }))).into_response()
}

/// Whether login is on, who is logged in, and where project previews are served.
pub async fn status(headers: HeaderMap) -> Json<serde_json::Value> {
    let preview = std::env::var("PREVIEW_ORIGIN").unwrap_or_default().trim_end_matches('/').to_string();
    match auth() {
        None => Json(json!({ "enabled": false, "user": null, "previewOrigin": preview })),
        Some(a) => {
            let ok = session_ok(a, &headers);
            Json(json!({ "enabled": true, "user": if ok { Some(a.user.clone()) } else { None }, "previewOrigin": preview }))
        }
    }
}

pub async fn login_page() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store"), (header::X_FRAME_OPTIONS, "DENY")],
        Html(LOGIN_HTML),
    )
}

const LOGIN_HTML: &str = r#"<!doctype html>
<html lang="vi"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Đăng nhập · AI Company</title>
<style>
:root{--bg:#1a1c2c;--card:#2c3352;--line:#14121f;--text:#f4f4f4;--muted:#94b0c2;--accent:#3b5dc9;--hi:#41a6f6;--red:#b13e53;--amber:#ffcd75}
*{box-sizing:border-box}html,body{height:100%}
body{margin:0;background:var(--bg);color:var(--text);font:18px/1.4 ui-monospace,"Cascadia Mono",monospace;display:grid;place-items:center;padding:16px;
background-image:linear-gradient(rgba(255,255,255,.03) 1px,transparent 1px),linear-gradient(90deg,rgba(255,255,255,.03) 1px,transparent 1px);background-size:16px 16px}
form{width:100%;max-width:360px;background:var(--card);border:3px solid var(--line);box-shadow:inset -3px -3px 0 rgba(0,0,0,.35),inset 3px 3px 0 rgba(255,255,255,.12),6px 6px 0 rgba(0,0,0,.4);padding:24px}
.brand{display:flex;gap:12px;align-items:center;margin-bottom:20px}.logo{background:#ef7d57;color:#1a1c2c;font-weight:bold;padding:6px 8px;border:3px solid var(--line)}
h1{font-size:22px;margin:0;color:var(--amber)}p{margin:2px 0 0;color:var(--muted);font-size:14px}
label{display:block;font-size:14px;color:var(--muted);margin:14px 0 4px}
input{width:100%;font:inherit;color:var(--text);background:#141626;border:3px solid var(--line);padding:8px 10px;outline:none}
input:focus{border-color:var(--hi)}
button{margin-top:20px;width:100%;font:inherit;font-weight:bold;color:#fff;background:var(--accent);border:3px solid var(--line);padding:10px;cursor:pointer;box-shadow:inset -3px -3px 0 rgba(0,0,0,.35),inset 3px 3px 0 rgba(255,255,255,.15)}
button:disabled{opacity:.6;cursor:wait}
.err{min-height:22px;margin-top:12px;color:#ff8a8a;font-size:14px}
</style></head><body>
<form id="f" autocomplete="on">
  <div class="brand"><span class="logo">AI</span><div><h1>AI Company</h1><p>Đăng nhập để vào văn phòng</p></div></div>
  <label for="u">Tài khoản</label><input id="u" name="username" autocomplete="username" required autofocus>
  <label for="p">Mật khẩu</label><input id="p" name="password" type="password" autocomplete="current-password" required>
  <button id="b" type="submit">Đăng nhập</button>
  <div class="err" id="e" role="alert"></div>
</form>
<script>
const f=document.getElementById('f'),b=document.getElementById('b'),e=document.getElementById('e');
f.addEventListener('submit',async(ev)=>{ev.preventDefault();b.disabled=true;e.textContent='';
  try{const r=await fetch('/api/auth/login',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({user:f.u.value,password:f.p.value})});
    if(r.ok){let n=new URLSearchParams(location.search).get('next')||'/';if(!n.startsWith('/')||n.startsWith('//'))n='/';location.replace(n);return}
    e.textContent=await r.text()||'Đăng nhập thất bại';f.p.value='';f.p.focus();
  }catch(x){e.textContent='Không kết nối được máy chủ'}finally{b.disabled=false}});
</script></body></html>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify() {
        let h = hash_password("Bí mật@1");
        let a = Auth { user: "u".into(), hash: h, secure_cookie: true, sessions: Mutex::new(HashMap::new()), fails: Mutex::new(HashMap::new()) };
        assert!(check(&a, "u", "Bí mật@1"));
        assert!(!check(&a, "u", "sai"));
        assert!(!check(&a, "x", "Bí mật@1"));
    }

    #[test]
    fn origin_and_cookie_parsing() {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, "1.2.3.4:8443".parse().unwrap());
        h.insert(header::ORIGIN, "https://1.2.3.4:8443".parse().unwrap());
        assert!(same_origin(&h));
        h.insert(header::ORIGIN, "https://1.2.3.4:8444".parse().unwrap());
        assert!(!same_origin(&h));
        h.insert(header::ORIGIN, "null".parse().unwrap());
        assert!(!same_origin(&h));
        h.insert(header::COOKIE, "a=1; aico_session=abc; b=2".parse().unwrap());
        assert_eq!(cookie_value(&h, COOKIE).as_deref(), Some("abc"));
        assert_eq!(urlencode("/a b?x"), "/a%20b%3Fx");
    }
}
