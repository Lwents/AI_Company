//! What each employee's computer shows, for the chairman to watch live: the file being written
//! (real content), commands and their output, the page open in the browser (real screenshots),
//! and what they are thinking. Kept in memory only (last few events per employee).
use crate::store::{now_ms, S};
use crate::types::ServerEvent;
use serde_json::{json, Value};
use std::path::Path;

const KEEP: usize = 40;
const MAX_CODE: usize = 120_000;
const MAX_OUTPUT: usize = 12_000;

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() > n { s.chars().take(n).collect::<String>() + "\n…" } else { s.to_string() }
}

fn push(store: &S, emp: &str, project: Option<&str>, mut v: Value) {
    v["ts"] = json!(now_ms());
    if let Some(p) = project {
        v["projectId"] = json!(p);
    }
    {
        let mut all = store.screens.lock().unwrap();
        let list = all.entry(emp.to_string()).or_default();
        // Only the newest screenshot is kept: older ones would just eat memory.
        if v.get("image").is_some_and(|i| !i.is_null()) {
            for old in list.iter_mut() {
                if let Some(o) = old.as_object_mut() {
                    o.remove("image");
                }
            }
        }
        list.push_back(v.clone());
        while list.len() > KEEP {
            list.pop_front();
        }
    }
    store.emit(ServerEvent::Screen { id: emp.to_string(), screen: v });
}

/// Someone has this employee's screen open (the page pings every few seconds).
pub fn watched(store: &S, emp: &str) -> bool {
    store.screen_watch.lock().unwrap().get(emp).is_some_and(|t| now_ms().saturating_sub(*t) < 30_000)
}

pub fn watch(store: &S, emp: &str) {
    store.screen_watch.lock().unwrap().insert(emp.to_string(), now_ms());
}

pub fn history(store: &S, emp: &str) -> Vec<Value> {
    store.screens.lock().unwrap().get(emp).map(|l| l.iter().cloned().collect()).unwrap_or_default()
}

/// Line range (1-based, inclusive) of `needle` inside `text`.
fn lines_of(text: &str, needle: &str) -> Option<(usize, usize)> {
    let needle = needle.trim_matches('\n');
    if needle.is_empty() {
        return None;
    }
    let at = text.find(needle)?;
    let start = text[..at].matches('\n').count() + 1;
    Some((start, start + needle.matches('\n').count()))
}

/// The editor: the whole file as it is on disk now, with the lines just typed / read highlighted.
pub fn code(store: &S, emp: &str, project: Option<&str>, dir: &Path, rel: &str, op: &str, input: &Value) {
    let Ok(path) = crate::tools::safe_path(dir, rel) else { return };
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let range = match op {
        "edit" => input.get("new_text").and_then(Value::as_str).and_then(|t| lines_of(&text, t)),
        "read" => {
            let a = input.get("start_line").and_then(Value::as_u64).map(|n| n as usize);
            let b = input.get("end_line").and_then(Value::as_u64).map(|n| n as usize);
            a.map(|a| (a, b.unwrap_or(a + 120)))
        }
        _ => None,
    };
    push(
        store,
        emp,
        project,
        json!({ "app": "code", "op": op, "path": rel.replace('\\', "/"), "content": cut(&text, MAX_CODE), "from": range.map(|r| r.0), "to": range.map(|r| r.1) }),
    );
}

pub fn terminal(store: &S, emp: &str, project: Option<&str>, command: &str, output: &str, ok: bool) {
    push(store, emp, project, json!({ "app": "terminal", "command": cut(command, 2000), "output": cut(output, MAX_OUTPUT), "ok": ok }));
}

/// The browser: current address, what was done, and the screenshot when there is one.
pub fn browser(store: &S, emp: &str, project: Option<&str>, action: &str, text: &str, input: &Value, image: Option<&(String, String)>) {
    let url = input
        .get("url")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| text.split("Page URL: ").nth(1).and_then(|s| s.lines().next()).map(|s| s.trim().to_string()));
    let title = text.split("Page Title: ").nth(1).and_then(|s| s.lines().next()).map(|s| s.trim().to_string());
    push(
        store,
        emp,
        project,
        json!({
            "app": "browser",
            "action": action,
            "url": url,
            "title": title,
            "image": image.map(|(mime, data)| format!("data:{mime};base64,{data}")),
            // Full-page screenshots map clicks to page coordinates, viewport ones to the visible area.
            "full": input.get("fullPage").and_then(Value::as_bool).unwrap_or(false),
        }),
    );
}

/// Checks a remote-control command from the chairman; returns it cleaned up, or why it was refused.
pub fn check_command(c: &Value) -> Result<Value, String> {
    let num = |k: &str| c.get(k).and_then(Value::as_f64).filter(|v| v.is_finite() && (-20_000.0..=20_000.0).contains(v));
    match c.get("kind").and_then(Value::as_str) {
        Some("click") => {
            let (x, y) = (num("x").ok_or("thiếu x")?, num("y").ok_or("thiếu y")?);
            Ok(json!({ "kind": "click", "x": x.round(), "y": y.round(), "full": c.get("full").and_then(Value::as_bool).unwrap_or(false) }))
        }
        Some("scroll") => Ok(json!({ "kind": "scroll", "dy": num("dy").ok_or("thiếu dy")?.round() })),
        Some("type") => {
            let t = c.get("text").and_then(Value::as_str).filter(|t| !t.is_empty() && t.chars().count() <= 500).ok_or("chữ trống hoặc quá dài")?;
            Ok(json!({ "kind": "type", "text": t }))
        }
        Some("key") => {
            let k = c.get("key").and_then(Value::as_str).unwrap_or_default();
            if k.is_empty() || k.len() > 20 || !k.chars().all(|ch| ch.is_ascii_alphanumeric()) {
                return Err("phím không hợp lệ".into());
            }
            Ok(json!({ "kind": "key", "key": k }))
        }
        Some("goto") => {
            let u = c.get("url").and_then(Value::as_str).map(str::trim).unwrap_or_default();
            if !(u.starts_with("http://") || u.starts_with("https://")) || u.len() > 2000 {
                return Err("chỉ mở được địa chỉ http:// hoặc https://".into());
            }
            Ok(json!({ "kind": "goto", "url": u }))
        }
        Some("back") => Ok(json!({ "kind": "back" })),
        _ => Err("lệnh không hợp lệ".into()),
    }
}

/// The value a browser_evaluate call returned (between "### Result" and the next section).
fn result_of(text: &str) -> String {
    let r = text.split("### Result").nth(1).unwrap_or("").split("\n###").next().unwrap_or("").trim();
    // Playwright prints the value as JSON ("span \"23:59\""): decode it when it is a string.
    let r = serde_json::from_str::<String>(r).unwrap_or_else(|_| r.to_string());
    r.chars().take(60).collect()
}

/// Plays the chairman's queued commands in the employee's own browser, then shows the page.
pub async fn run_remote(store: &S, emp: &str, project: Option<&str>, m: &mut crate::mcp::Session, dir: &Path) {
    let cmds = store.remote.lock().unwrap().remove(emp).unwrap_or_default();
    if cmds.is_empty() || !m.has("browser__browser_evaluate") {
        return;
    }
    let mut did: Vec<String> = vec![];
    for c in &cmds {
        let js = |f: String| json!({ "function": f });
        match c["kind"].as_str().unwrap_or_default() {
            "click" => {
                let (x, y, full) = (c["x"].as_f64().unwrap_or(0.0), c["y"].as_f64().unwrap_or(0.0), c["full"].as_bool().unwrap_or(false));
                let f = format!(
                    "() => {{ let x = {x}, y = {y}; if ({full}) {{ window.scrollTo(0, Math.max(0, y - innerHeight / 2)); y = y - scrollY; }} \
                     const el = document.elementFromPoint(x, y); if (!el) return 'chỗ trống'; if (el.focus) el.focus(); el.click(); \
                     const t = (el.innerText || el.value || el.getAttribute('aria-label') || '').trim().slice(0, 30); \
                     return el.tagName.toLowerCase() + (t ? ' \"' + t + '\"' : ''); }}"
                );
                let r = m.call("browser__browser_evaluate", &js(f)).await;
                did.push(format!("bấm {}", result_of(&r.text)));
            }
            "scroll" => {
                let dy = c["dy"].as_f64().unwrap_or(0.0);
                let _ = m.call("browser__browser_evaluate", &js(format!("() => {{ window.scrollBy(0, {dy}); return Math.round(scrollY); }}"))).await;
                did.push(if dy > 0.0 { "cuộn xuống".into() } else { "cuộn lên".into() });
            }
            "type" => {
                let t = serde_json::to_string(c["text"].as_str().unwrap_or_default()).unwrap_or_default();
                let f = format!(
                    "() => {{ const e = document.activeElement; const t = {t}; \
                     if (e && 'value' in e) {{ const s = e.selectionStart ?? e.value.length; const en = e.selectionEnd ?? s; e.value = e.value.slice(0, s) + t + e.value.slice(en); \
                     e.dispatchEvent(new Event('input', {{ bubbles: true }})); return 'ok'; }} \
                     if (e && e.isContentEditable) {{ document.execCommand('insertText', false, t); return 'ok'; }} return 'chưa chọn ô nhập'; }}"
                );
                let r = m.call("browser__browser_evaluate", &js(f)).await;
                did.push(format!("gõ \"{}\" ({})", c["text"].as_str().unwrap_or_default().chars().take(20).collect::<String>(), result_of(&r.text)));
            }
            "key" => {
                let k = c["key"].as_str().unwrap_or_default();
                let _ = m.call("browser__browser_press_key", &json!({ "key": k })).await;
                did.push(format!("phím {k}"));
            }
            "goto" => {
                let u = c["url"].as_str().unwrap_or_default();
                let _ = m.call("browser__browser_navigate", &json!({ "url": u })).await;
                did.push(format!("mở {u}"));
            }
            "back" => {
                let _ = m.call("browser__browser_navigate_back", &json!({})).await;
                did.push("quay lại".into());
            }
            _ => {}
        }
    }
    let shot = m.call("browser__browser_take_screenshot", &json!({ "type": "jpeg" })).await;
    if let Some(rel) = shot.text.split("path: '").nth(1).and_then(|s| s.split('\'').next()) {
        if let Ok(f) = crate::tools::safe_path(dir, &rel.replace("\\\\", "/").replace('\\', "/")) {
            let _ = std::fs::remove_file(f);
        }
    }
    browser(store, emp, project, &format!("👑 Chủ tịch: {}", did.join(", ")), &shot.text, &json!({}), shot.images.first());
    // Let the employee know (at most every 2 minutes): the page they are testing may have changed.
    let now = now_ms();
    let tell = {
        let mut told = store.remote_told.lock().unwrap();
        let due = told.get(emp).is_none_or(|t| now.saturating_sub(*t) > 120_000);
        if due {
            told.insert(emp.to_string(), now);
        }
        due
    };
    if tell {
        store.post_message(
            project,
            "system",
            emp,
            format!("(Chủ tịch vừa tự thao tác trên trình duyệt của bạn: {}.) Trang có thể đã thay đổi: chụp / đọc lại trang trước khi kết luận.", did.join(", ")),
            "chat",
        );
    }
}

/// A thought / short note from the model between tool calls.
pub fn note(store: &S, emp: &str, project: Option<&str>, text: &str) {
    let t = text.trim();
    if !t.is_empty() {
        push(store, emp, project, json!({ "app": "note", "text": cut(t, 1500) }));
    }
}

#[cfg(test)]
mod remote_tests {
    use super::check_command;
    use serde_json::json;

    #[test]
    fn accepts_safe_commands_only() {
        assert_eq!(check_command(&json!({ "kind": "click", "x": 10.4, "y": 20.6 })).unwrap()["x"], 10.0);
        assert!(check_command(&json!({ "kind": "scroll", "dy": 300 })).is_ok());
        assert!(check_command(&json!({ "kind": "goto", "url": "https://example.com" })).is_ok());
        assert!(check_command(&json!({ "kind": "goto", "url": "file:///etc/passwd" })).is_err());
        assert!(check_command(&json!({ "kind": "goto", "url": "javascript:alert(1)" })).is_err());
        assert!(check_command(&json!({ "kind": "key", "key": "Enter" })).is_ok());
        assert!(check_command(&json!({ "kind": "key", "key": "a'); x" })).is_err());
        assert!(check_command(&json!({ "kind": "type", "text": "" })).is_err());
        assert!(check_command(&json!({ "kind": "type", "text": "x".repeat(600) })).is_err());
        assert!(check_command(&json!({ "kind": "click", "x": "1" })).is_err());
        assert!(check_command(&json!({ "kind": "run", "cmd": "rm -rf /" })).is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::lines_of;

    #[test]
    fn finds_edited_lines() {
        let text = "a\nb\nc\nd\n";
        assert_eq!(lines_of(text, "c\nd"), Some((3, 4)));
        assert_eq!(lines_of(text, "b"), Some((2, 2)));
        assert_eq!(lines_of(text, "zzz"), None);
    }
}
