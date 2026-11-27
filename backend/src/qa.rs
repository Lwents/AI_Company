//! Machine checks before a product is handed over — no model involved, so nothing is "assumed fine".
//! A real browser opens the preview at desktop and phone width, walks the home page and its internal
//! links, and reports what a user would hit: missing files (4xx / 5xx), console errors, buttons whose
//! text can't be seen (text colour ≈ background), and pages wider than the screen.
use crate::store::S;
use crate::types::*;
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// Inspects the page in the browser; returns JSON text {invisible: [...], overflow: px, links: [...]}.
const PAGE_CHECK: &str = r##"() => {
  const rgb = (c) => { const m = (c || "").match(/rgba?\(([^)]+)\)/); if (!m) return null; const p = m[1].split(",").map(Number); return { r: p[0], g: p[1], b: p[2], a: p.length > 3 ? p[3] : 1 }; };
  const lum = (c) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); }; return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b); };
  const bgOf = (el) => { for (let n = el; n; n = n.parentElement) { const s = getComputedStyle(n); if (s.backgroundImage && s.backgroundImage !== "none") return null; const c = rgb(s.backgroundColor); if (c && c.a > 0.5) return c; } return { r: 255, g: 255, b: 255, a: 1 }; };
  const invisible = [];
  for (const el of document.querySelectorAll("button, a[href], [role=button], input[type=submit], input[type=button]")) {
    const r = el.getBoundingClientRect(); const s = getComputedStyle(el);
    if (r.width < 8 || r.height < 8 || s.visibility === "hidden" || s.display === "none" || Number(s.opacity) < 0.1) continue;
    const text = (el.innerText || el.value || "").trim(); if (!text) continue;
    const fg = rgb(s.color); const bg = bgOf(el); if (!fg || !bg) continue;
    const a = lum(fg), b = lum(bg); const ratio = (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
    if (ratio < 1.6) invisible.push(text.replace(/\s+/g, " ").slice(0, 40));
  }
  const here = location.origin + location.pathname.replace(/[^/]*$/, "");
  const links = [...document.querySelectorAll("a[href]")].map((a) => a.href.split("#")[0]).filter((h) => h.startsWith(here));
  return JSON.stringify({ invisible, overflow: document.documentElement.scrollWidth - innerWidth, links: [...new Set(links)].slice(0, 8) });
}"##;

/// "=> [404] Not Found" style status codes ≥ 400 in Playwright's network listing, with their URL.
fn failed_requests(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let i = l.find("=> [")?;
            let code: u16 = l[i + 4..].get(..3)?.parse().ok()?;
            (code >= 400).then(|| {
                let url = l.split_whitespace().find(|w| w.starts_with("http")).unwrap_or(l.trim());
                format!("{code} {}", url.chars().take(140).collect::<String>())
            })
        })
        .collect()
}

fn console_errors(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("[ERROR]") || l.starts_with("- [ERROR]") || l.to_lowercase().starts_with("[error]"))
        // A failed resource is already reported with its URL.
        .filter(|l| !l.contains("Failed to load resource"))
        .map(|l| l.chars().take(200).collect())
        .collect()
}

fn page_name(url: &str, base: &str) -> String {
    let rest = url.strip_prefix(base).unwrap_or(url);
    if rest.is_empty() { "trang chủ".into() } else { format!("/{rest}") }
}

/// Problems a user would see on the product's preview; empty when it looks fine (or there is no web UI).
pub async fn smoke(store: &S, p: &Project) -> Vec<String> {
    let dir = store.dir_of(p);
    if !crate::workflow::has_preview(&dir) {
        return vec![];
    }
    let root = store.data_dir.parent().map(|x| x.to_path_buf()).unwrap_or_default();
    let overrides = store.with(|s| s.settings.mcp.clone());
    let mut m = crate::mcp::Session::start(&root, &dir, Role::Reviewer, "review", &overrides).await;
    if !m.has("browser__browser_navigate") || !m.has("browser__browser_evaluate") {
        return vec![];
    }
    let base = crate::config::preview_url(&p.id);
    let mut issues: BTreeSet<String> = BTreeSet::new();
    let mut pages = vec![base.clone()];
    for (w, h, label) in [(1280, 900, "máy tính"), (375, 812, "điện thoại")] {
        let _ = m.call("browser__browser_resize", &json!({ "width": w, "height": h })).await;
        let mut i = 0;
        while i < pages.len() && i < 6 {
            let url = pages[i].clone();
            i += 1;
            let nav = m.call("browser__browser_navigate", &json!({ "url": url })).await;
            if nav.is_error {
                issues.insert(format!("Không mở được {}: {}", page_name(&url, &base), nav.text.chars().take(120).collect::<String>()));
                continue;
            }
            let _ = m.call("browser__browser_wait_for", &json!({ "time": 1.5 })).await;
            let name = page_name(&url, &base);
            let r = m.call("browser__browser_evaluate", &json!({ "function": PAGE_CHECK })).await;
            let raw = r.text.split("### Result").nth(1).unwrap_or("").split("\n###").next().unwrap_or("").trim().to_string();
            let data: Value = serde_json::from_str::<String>(&raw).ok().and_then(|s| serde_json::from_str(&s).ok()).or_else(|| serde_json::from_str(&raw).ok()).unwrap_or(Value::Null);
            for t in data["invisible"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                issues.insert(format!("Nút \"{t}\" không nhìn thấy được (chữ trùng màu nền) · {name}"));
            }
            if let Some(o) = data["overflow"].as_i64().filter(|o| *o > 2) {
                issues.insert(format!("Trang rộng hơn màn hình {o}px, bị tràn ngang ({label} {w}px) · {name}"));
            }
            if w == 1280 {
                for l in data["links"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                    if !pages.iter().any(|x| x == l) && pages.len() < 6 {
                        pages.push(l.to_string());
                    }
                }
            }
            let net = m.call("browser__browser_network_requests", &json!({})).await;
            for f in failed_requests(&net.text) {
                issues.insert(format!("Tải tệp lỗi {f} · {name}"));
            }
            let con = m.call("browser__browser_console_messages", &json!({})).await;
            for e in console_errors(&con.text) {
                issues.insert(format!("Lỗi JavaScript: {e} · {name}"));
            }
        }
    }
    issues.into_iter().take(25).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_failed_requests_and_errors() {
        let net = "[GET] https://x/preview/p/ => [200] OK\n[GET] https://x/sounds/move.wav => [404] Not Found\n[GET] https://x/api => [500] Internal";
        assert_eq!(failed_requests(net), vec!["404 https://x/sounds/move.wav", "500 https://x/api"]);
        let con = "- [ERROR] Uncaught TypeError: a is undefined\n- [LOG] ok\n- [ERROR] Failed to load resource: 404";
        assert_eq!(console_errors(con).len(), 1);
        assert_eq!(page_name("http://h/preview/p/play/local", "http://h/preview/p/"), "/play/local");
        assert_eq!(page_name("http://h/preview/p/", "http://h/preview/p/"), "trang chủ");
    }
}
