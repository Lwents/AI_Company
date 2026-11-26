//! Git like a real software company: every project is a repository, the team works on its own
//! branch, and each finished ticket / phase is a commit. Never pushes anywhere.
use crate::store::S;
use crate::types::Project;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const AUTHOR: [&str; 4] = ["-c", "user.name=AI Company", "-c", "user.email=ai-company@localhost"];
const IGNORE: [&str; 6] = ["node_modules/", "dist/", ".playwright-mcp/", ".env", "coverage/", ".DS_Store"];

/// One git operation at a time per project (parallel devs share a working tree).
fn lock(pid: &str) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(Default::default).lock().unwrap().entry(pid.to_string()).or_default().clone()
}

async fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(AUTHOR).args(args).current_dir(dir).env("GIT_TERMINAL_PROMPT", "0").kill_on_drop(true);
    if let Some(parent) = dir.parent() {
        cmd.env("GIT_CEILING_DIRECTORIES", parent);
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let out = tokio::time::timeout(Duration::from_secs(120), cmd.output())
        .await
        .map_err(|_| "git quá thời gian".to_string())?
        .map_err(|e| format!("không chạy được git: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    if out.status.success() {
        Ok(text)
    } else {
        Err(text.trim().chars().take(300).collect())
    }
}

pub fn branch_name(p: &Project) -> String {
    format!("ai-company/{}", p.slug)
}

/// Makes sure the project folder is a repository on the team's branch. Idempotent.
pub async fn prepare(store: &S, p: &Project) {
    let dir = store.dir_of(p);
    let l = lock(&p.id);
    let _g = l.lock().await;
    if !dir.join(".git").exists() {
        if let Err(e) = git(&dir, &["init", "-q", "-b", "main"]).await {
            store.log_activity("system", Some(&p.id), "error", format!("git init: {e}"));
            return;
        }
    }
    // Keep dependencies, build output and secrets out of history.
    let gi = dir.join(".gitignore");
    let current = std::fs::read_to_string(&gi).unwrap_or_default();
    let missing: Vec<&str> = IGNORE.iter().copied().filter(|x| !current.lines().any(|l| l.trim() == *x || l.trim() == x.trim_end_matches('/'))).collect();
    if !missing.is_empty() {
        let mut s = current.clone();
        if !s.is_empty() && !s.ends_with('\n') {
            s.push('\n');
        }
        s.push_str(&missing.join("\n"));
        s.push('\n');
        let _ = std::fs::write(&gi, s);
    }
    // A repository without any commit yet: record the starting point.
    if git(&dir, &["rev-parse", "--verify", "HEAD"]).await.is_err() {
        let _ = git(&dir, &["add", "-A"]).await;
        let _ = git(&dir, &["commit", "-q", "--allow-empty", "-m", "Khởi tạo dự án"]).await;
    }
    // The customer's own repository: work on a separate branch, their branches stay untouched.
    if !p.dir.is_empty() {
        let b = branch_name(p);
        let on = git(&dir, &["rev-parse", "--abbrev-ref", "HEAD"]).await.unwrap_or_default();
        if on.trim() != b {
            let r = if git(&dir, &["rev-parse", "--verify", &b]).await.is_ok() { git(&dir, &["checkout", "-q", &b]).await } else { git(&dir, &["checkout", "-q", "-b", &b]).await };
            match r {
                Ok(_) => {
                    store.post_message(Some(&p.id), "system", "all", format!("Đội làm việc trên nhánh Git riêng: {b}"), "chat");
                }
                Err(e) => store.log_activity("system", Some(&p.id), "error", format!("git checkout {b}: {e}")),
            };
        }
    }
}

/// Commits everything that changed. Silent no-op when nothing changed or git is unavailable.
pub async fn commit(store: &S, pid: &str, message: &str) {
    let Some(p) = store.project(pid) else { return };
    let dir = store.dir_of(&p);
    if !dir.join(".git").exists() {
        return;
    }
    let l = lock(pid);
    let _g = l.lock().await;
    if git(&dir, &["add", "-A"]).await.is_err() {
        return;
    }
    if git(&dir, &["diff", "--cached", "--quiet"]).await.is_ok() {
        return; // nothing staged
    }
    match git(&dir, &["commit", "-q", "-m", message]).await {
        Ok(_) => {
            let hash = git(&dir, &["rev-parse", "--short", "HEAD"]).await.unwrap_or_default();
            store.log_activity("system", Some(pid), "result", format!("git commit {}: {}", hash.trim(), message.lines().next().unwrap_or("")));
        }
        Err(e) => store.log_activity("system", Some(pid), "error", format!("git commit lỗi: {e}")),
    }
}
