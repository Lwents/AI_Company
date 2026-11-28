//! The system maintainer: Claude CLI working on AI Company's own source code. The chairman chats with it the
//! way he would with Claude Code — it reads, fixes, checks, commits and pushes (a push to main deploys the
//! server through GitHub Actions). It only works inside the source folder, as the service user (no sudo).
use crate::store::S;
use crate::types::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio_util::sync::CancellationToken;

/// One request may take long (reading, building, fixing), but never forever.
const MAX_RUN: Duration = Duration::from_secs(60 * 60);

fn queue() -> &'static tokio::sync::Mutex<()> {
    static Q: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    Q.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn current() -> &'static Mutex<Option<CancellationToken>> {
    static C: OnceLock<Mutex<Option<CancellationToken>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

/// The AI Company source folder (a git clone): `AICO_SRC_DIR` on the server, else the app folder when it is a repo.
pub fn src_dir(store: &S) -> Option<PathBuf> {
    if let Ok(d) = std::env::var("AICO_SRC_DIR") {
        let p = PathBuf::from(d.trim());
        return p.join(".git").exists().then_some(p);
    }
    let root = store.data_dir.parent()?.to_path_buf();
    root.join(".git").exists().then_some(root)
}

fn session_file(store: &S) -> PathBuf {
    store.data_dir.join("maintainer.json")
}

fn load_session(store: &S) -> Option<String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(session_file(store)).ok()?).ok()?;
    v.get("session").and_then(Value::as_str).map(str::to_string).filter(|s| !s.is_empty())
}

fn save_session(store: &S, id: Option<&str>) {
    let _ = std::fs::write(session_file(store), json!({ "session": id.unwrap_or("") }).to_string());
}

fn has_cargo() -> bool {
    std::process::Command::new("cargo").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn rules(dir: &Path) -> String {
    let cargo = if has_cargo() {
        "Có cargo: sửa Rust thì chạy `cargo test --release --manifest-path backend/Cargo.toml`."
    } else {
        "Máy này KHÔNG có cargo: phần Rust do GitHub Actions build + test trước khi deploy (lỗi thì không deploy). Sửa Rust thật cẩn thận, đọc kỹ code xung quanh, giữ đúng kiểu dữ liệu."
    };
    format!(
        "Bạn là nhân viên BẢO TRÌ HỆ THỐNG của AI Company, làm việc trực tiếp trên mã nguồn của chính hệ thống AI Company tại {dir} (repo git, remote origin trên GitHub, nhánh main). Chủ tịch nhắn yêu cầu qua chat (thường từ điện thoại); làm đúng yêu cầu, trả lời ngắn gọn bằng tiếng Việt, xưng \"em\" gọi \"anh/chị\".\n\
PHẠM VI: CHỈ đọc / sửa trong thư mục mã nguồn này. Không đụng hệ điều hành, nginx, systemd, các dịch vụ khác trên máy (9router, xoay-opencode-api, lwent.cloud), không dùng sudo, không đọc hay in ra bí mật (khóa API, mật khẩu, tệp .env).\n\
CẤU TRÚC: backend Rust (backend/src), giao diện React + Vite (web/src), kỹ năng nhân viên (skills/), triển khai (.github/workflows/deploy.yml, deploy/). Dữ liệu đang chạy và dự án của khách KHÔNG ở đây — không commit data/, workspace/, .env.\n\
CÁCH LÀM: đầu mỗi việc `git pull --rebase origin main`. Kiểm tra trước khi push: `npm ci` (nếu chưa có node_modules) rồi `npm run typecheck && npm run build`. {cargo}\n\
XONG VIỆC SỬA: commit (message tiếng Việt, rõ ràng) rồi `git push origin main` — trừ khi Chủ tịch dặn chưa push. Push lên main thì GitHub Actions tự build, test và deploy lên server trong khoảng 5–10 phút, dịch vụ tự khởi động lại; push xong thì báo lại ngắn gọn (đã sửa gì, commit nào) và KẾT THÚC lượt, không chờ deploy.",
        dir = dir.display()
    )
}

/// Answers a chat message to the maintainer: runs Claude CLI on it and posts what it says.
pub async fn handle(store: &S, msg: &ChatMsg) -> Result<(), String> {
    let emp = store.employee(&msg.to).ok_or("Không có nhân viên")?;
    let pid = msg.project_id.clone();
    let say = |text: &str| {
        store.post_message(pid.as_deref(), &emp.id, "chairman", text.to_string(), "chat");
    };
    if !emp.enabled {
        say("Em đang tắt. Anh/chị bật em ở tab Nhân sự rồi nhắn lại nhé.");
        return Ok(());
    }
    let text = msg.text.trim();
    let lower = text.to_lowercase();
    if ["dừng", "dung", "stop", "hủy", "huỷ", "dừng lại"].contains(&lower.as_str()) {
        let stopped = current().lock().unwrap().as_ref().map(|t| t.cancel()).is_some();
        say(if stopped { "⏹ Em đã dừng việc đang làm." } else { "Em không có việc nào đang chạy." });
        return Ok(());
    }
    if ["phiên mới", "phien moi", "/clear", "/new"].contains(&lower.as_str()) {
        save_session(store, None);
        say("🆕 Đã mở phiên làm việc mới (em bắt đầu lại, không nhớ trao đổi cũ).");
        return Ok(());
    }
    let Some(dir) = src_dir(store) else {
        say("Máy chủ này chưa có thư mục mã nguồn AI Company (AICO_SRC_DIR) nên em chưa sửa được hệ thống.");
        return Ok(());
    };
    let guard = match queue().try_lock() {
        Ok(g) => g,
        Err(_) => {
            say("⏳ Em đang làm việc trước, xong em làm tiếp yêu cầu này ngay.");
            queue().lock().await
        }
    };
    let r = run(store, &emp, pid.as_deref(), &dir, text).await;
    drop(guard);
    store.set_runtime(&emp.id, EmpStatus::Idle, "", None, None);
    if let Err(e) = r {
        say(&format!("⚠️ {e}"));
    }
    Ok(())
}

fn short(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() > n { s.chars().take(n).collect::<String>() + "…" } else { s.to_string() }
}

/// A one-line label for what Claude CLI is doing.
fn describe(name: &str, input: &Value) -> String {
    let g = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match name {
        "Bash" => format!("$ {}", short(&g("command"), 140)),
        "Read" => format!("Đọc {}", g("file_path")),
        "Edit" | "MultiEdit" => format!("Sửa {}", g("file_path")),
        "Write" => format!("Ghi {}", g("file_path")),
        "Grep" => format!("Tìm \"{}\"", short(&g("pattern"), 60)),
        "Glob" => format!("Tìm tệp {}", g("pattern")),
        "WebSearch" => format!("Tìm trên web: {}", short(&g("query"), 60)),
        "WebFetch" => format!("Đọc trang: {}", short(&g("url"), 80)),
        "TodoWrite" => "Lập danh sách việc".into(),
        other => other.to_string(),
    }
}

fn command(dir: &Path, text: &str, session: Option<&str>, emp: &Employee) -> tokio::process::Command {
    let bin = std::env::var("AICO_CLAUDE_BIN").unwrap_or_else(|_| "claude".into());
    let mut cmd = tokio::process::Command::new(bin);
    cmd.arg("-p").arg(text).args(["--output-format", "stream-json", "--verbose", "--dangerously-skip-permissions"]);
    cmd.arg("--append-system-prompt").arg(rules(dir));
    if let Some(s) = session {
        cmd.arg("--resume").arg(s);
    }
    if let Ok(m) = std::env::var("AICO_CLAUDE_MODEL") {
        cmd.arg("--model").arg(m);
    }
    cmd.current_dir(dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    // On the server the CLI gets only what it needs (the service's secrets stay out of its reach).
    if let Ok(home) = std::env::var("AICO_CLAUDE_HOME") {
        let path = std::env::var("PATH").unwrap_or_default();
        let _ = std::fs::create_dir_all(&home);
        cmd.env_clear().env("PATH", path).env("HOME", &home).env("LANG", "C.UTF-8").env("TERM", "dumb");
    }
    // Not a child of whatever Claude Code session started this server.
    for (k, _) in std::env::vars() {
        if k.starts_with("CLAUDE_CODE_") {
            cmd.env_remove(k);
        }
    }
    if let Ok(url) = std::env::var("AICO_CLAUDE_BASE_URL") {
        cmd.env("ANTHROPIC_BASE_URL", url);
        if let Ok(key) = std::env::var("AICO_CLAUDE_KEY").or_else(|_| std::env::var("CUSTOM_API_KEY")) {
            cmd.env("ANTHROPIC_API_KEY", key);
        }
    }
    if let Ok(m) = std::env::var("AICO_CLAUDE_SMALL_MODEL") {
        cmd.env("ANTHROPIC_DEFAULT_HAIKU_MODEL", &m).env("ANTHROPIC_SMALL_FAST_MODEL", &m);
    }
    cmd.env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1");
    let who = format!("{} (Bảo trì AI Company)", emp.name);
    cmd.env("GIT_AUTHOR_NAME", &who).env("GIT_COMMITTER_NAME", &who).env("GIT_AUTHOR_EMAIL", "maintainer@ai-company.local").env("GIT_COMMITTER_EMAIL", "maintainer@ai-company.local");
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    cmd
}

async fn run(store: &S, emp: &Employee, pid: Option<&str>, dir: &Path, text: &str) -> Result<(), String> {
    let say = |t: &str| {
        store.post_message(pid, &emp.id, "chairman", t.to_string(), "chat");
    };
    store.set_runtime(&emp.id, EmpStatus::Working, "Claude CLI đang đọc yêu cầu", None, None);
    let session = load_session(store);
    let mut child = command(dir, text, session.as_deref(), emp).spawn().map_err(|e| format!("Không chạy được Claude CLI: {e}"))?;
    let stop = CancellationToken::new();
    *current().lock().unwrap() = Some(stop.clone());
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut stderr = child.stderr.take().unwrap();
    let err_task = tokio::spawn(async move {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s).await;
        s
    });
    let started = std::time::Instant::now();
    let mut last_text = String::new();
    let mut result: Option<Value> = None;
    let deadline = tokio::time::sleep(MAX_RUN);
    tokio::pin!(deadline);
    let mut stopped = None;
    loop {
        let line = tokio::select! {
            l = lines.next_line() => l,
            _ = stop.cancelled() => { stopped = Some("Chủ tịch bảo dừng"); break; }
            _ = &mut deadline => { stopped = Some("quá 60 phút"); break; }
        };
        let Ok(Some(line)) = line else { break };
        let Ok(ev) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(sid) = ev.get("session_id").and_then(Value::as_str) {
            if session.as_deref() != Some(sid) {
                save_session(store, Some(sid));
            }
        }
        match ev.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                for c in ev.pointer("/message/content").and_then(Value::as_array).cloned().unwrap_or_default() {
                    match c.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let t = c.get("text").and_then(Value::as_str).unwrap_or("").trim().to_string();
                            if !t.is_empty() {
                                say(&t);
                                crate::screen::note(store, &emp.id, pid, &t);
                                last_text = t;
                            }
                        }
                        Some("tool_use") => {
                            let label = describe(c.get("name").and_then(Value::as_str).unwrap_or(""), c.get("input").unwrap_or(&Value::Null));
                            store.log_activity(&emp.id, pid, "tool", &label);
                            store.set_runtime(&emp.id, EmpStatus::Working, short(&label, 120), None, None);
                            crate::screen::note(store, &emp.id, pid, &label);
                        }
                        _ => {}
                    }
                }
            }
            Some("result") => {
                result = Some(ev);
                break;
            }
            _ => {}
        }
    }
    *current().lock().unwrap() = None;
    if stopped.is_some() {
        let _ = child.kill().await;
    }
    let status = tokio::time::timeout(Duration::from_secs(20), child.wait()).await.ok().and_then(Result::ok);
    let err = err_task.await.unwrap_or_default();
    let mins = started.elapsed().as_secs_f64() / 60.0;
    if let Some(why) = stopped {
        say(&format!("⏹ Đã dừng ({why}) sau {mins:.1} phút. Phần đã sửa vẫn nằm trong thư mục mã nguồn (chưa push nếu em chưa kịp push)."));
        return Ok(());
    }
    let Some(r) = result else {
        let tail: String = err.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
        return Err(format!("Claude CLI dừng giữa chừng{}{}", status.map(|s| format!(" (mã {})", s.code().unwrap_or(-1))).unwrap_or_default(), if tail.is_empty() { String::new() } else { format!(":\n{tail}") }));
    };
    let inp = r.pointer("/usage/input_tokens").and_then(Value::as_u64).unwrap_or(0)
        + r.pointer("/usage/cache_read_input_tokens").and_then(Value::as_u64).unwrap_or(0)
        + r.pointer("/usage/cache_creation_input_tokens").and_then(Value::as_u64).unwrap_or(0);
    let out = r.pointer("/usage/output_tokens").and_then(Value::as_u64).unwrap_or(0);
    store.add_usage(&emp.id, pid, inp, out);
    let final_text = r.get("result").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let failed = r.get("is_error").and_then(Value::as_bool).unwrap_or(false);
    let footer = format!("⏱ {mins:.1} phút");
    if failed {
        say(&format!("⚠️ Claude CLI báo lỗi: {}\n{footer}", short(if final_text.is_empty() { r.get("subtype").and_then(Value::as_str).unwrap_or("") } else { &final_text }, 1500)));
    } else if !final_text.is_empty() && final_text != last_text {
        say(&format!("{final_text}\n\n{footer}"));
    } else {
        say(&format!("✅ Xong. {footer}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_cli_actions() {
        assert_eq!(describe("Bash", &json!({ "command": "git push origin main" })), "$ git push origin main");
        assert_eq!(describe("Edit", &json!({ "file_path": "web/src/App.tsx" })), "Sửa web/src/App.tsx");
        assert!(rules(Path::new("/opt/src")).contains("git push origin main"));
    }
}
