//! "Tài nguyên": reads the AI accounts and their quotas from one or more 9router dashboards,
//! so the chairman can see how much is left without opening 9router.
//!
//! Config (.env):
//!   NINEROUTER_URL       comma-separated, e.g. "https://1.2.3.4,http://127.0.0.1:20128" (default: this machine)
//!   NINEROUTER_PASSWORD  dashboard password (one for all, or comma-separated in the same order)
//! Only whitelisted, non-secret fields are passed to the browser (no tokens).
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

static COOKIES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn bases() -> Vec<String> {
    let raw = std::env::var("NINEROUTER_URL").unwrap_or_else(|_| "http://127.0.0.1:20128".into());
    raw.split(',').map(|s| s.trim().trim_end_matches('/').to_string()).filter(|s| !s.is_empty()).collect()
}

fn password(idx: usize) -> Option<String> {
    let raw = std::env::var("NINEROUTER_PASSWORD").ok()?;
    let parts: Vec<&str> = raw.split(',').collect();
    // A single password (which may itself contain no comma) applies to every source.
    let p = if parts.len() > 1 { parts.get(idx).copied()? } else { raw.as_str() };
    Some(p.trim().to_string()).filter(|p| !p.is_empty())
}

fn label(base: &str) -> String {
    let host = base.split("://").nth(1).unwrap_or(base).split(['/', ':']).next().unwrap_or(base);
    if host == "127.0.0.1" || host == "localhost" {
        "Máy này".into()
    } else {
        format!("VPS {host}")
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        // A self-hosted 9router (VPS) often uses a self-signed certificate.
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap_or_default()
}

async fn login(c: &reqwest::Client, idx: usize, base: &str) -> Result<String, String> {
    let pw = password(idx).ok_or("Chưa đặt NINEROUTER_PASSWORD trong .env (mật khẩu đăng nhập 9router).")?;
    let r = c
        .post(format!("{base}/api/auth/login"))
        .json(&json!({ "password": pw }))
        .send()
        .await
        .map_err(|e| format!("Không kết nối được 9router ({base}): {e}"))?;
    if !r.status().is_success() {
        return Err(format!("Đăng nhập 9router ({base}) thất bại (HTTP {}). Kiểm tra NINEROUTER_PASSWORD.", r.status().as_u16()));
    }
    let cookie = r
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|v| v.split(';').next().filter(|kv| kv.starts_with("auth_token=")).map(str::to_string))
        .ok_or("9router không trả về phiên đăng nhập")?;
    COOKIES.lock().unwrap().get_or_insert_with(HashMap::new).insert(base.to_string(), cookie.clone());
    Ok(cookie)
}

/// GET a dashboard path on source `idx`, logging in (again) when needed.
async fn get(idx: usize, path: &str) -> Result<Value, String> {
    let base = bases().get(idx).cloned().ok_or("Nguồn 9router không tồn tại")?;
    let c = client();
    let cached = COOKIES.lock().unwrap().as_ref().and_then(|m| m.get(&base).cloned());
    for attempt in 0..2 {
        let cookie = match (&cached, attempt) {
            (Some(k), 0) => k.clone(),
            _ => login(&c, idx, &base).await?,
        };
        let r = c
            .get(format!("{base}{path}"))
            .header(reqwest::header::COOKIE, cookie)
            .send()
            .await
            .map_err(|e| format!("Không kết nối được 9router ({base}): {e}"))?;
        if r.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
            continue;
        }
        if !r.status().is_success() {
            return Err(format!("9router trả lỗi HTTP {}", r.status().as_u16()));
        }
        return r.json().await.map_err(|e| format!("9router trả dữ liệu lạ: {e}"));
    }
    Err("Đăng nhập 9router thất bại".into())
}

async fn accounts_of(idx: usize) -> Result<Vec<Value>, String> {
    let v = get(idx, "/api/providers").await?;
    Ok(v["connections"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|c| {
            json!({
                "id": c["id"], "provider": c["provider"], "name": c["name"], "email": c["email"],
                "authType": c["authType"], "isActive": c["isActive"], "priority": c["priority"],
                "testStatus": c["testStatus"], "lastError": c["lastError"], "lastErrorAt": c["lastErrorAt"],
                "rateLimitedUntil": c["rateLimitedUntil"],
            })
        })
        .collect())
}

/// Every configured 9router with its accounts (or the error reaching it).
pub async fn sources() -> Value {
    let list = bases();
    let results = futures_join(list.len()).await;
    let out: Vec<Value> = list
        .iter()
        .zip(results)
        .enumerate()
        .map(|(i, (b, r))| match r {
            Ok(acc) => json!({ "idx": i, "url": b, "label": label(b), "accounts": acc }),
            Err(e) => json!({ "idx": i, "url": b, "label": label(b), "accounts": [], "error": e }),
        })
        .collect();
    json!({ "sources": out })
}

async fn futures_join(n: usize) -> Vec<Result<Vec<Value>, String>> {
    let handles: Vec<_> = (0..n).map(|i| tokio::spawn(accounts_of(i))).collect();
    let mut out = Vec::with_capacity(n);
    for h in handles {
        out.push(h.await.unwrap_or_else(|e| Err(e.to_string())));
    }
    out
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Quota of one account: plan + the pooled limits (5h / weekly).
pub async fn quota(idx: usize, id: &str) -> Result<Value, String> {
    let v = get(idx, &format!("/api/usage/{id}")).await?;
    let sub = &v["subscriptionInfo"];
    let tier = sub["paidTier"]["name"].as_str().or(sub["currentTier"]["name"].as_str()).map(str::to_string);
    let mut rows: Vec<Value> = v["quotas"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, q)| {
                    json!({
                        "model": k, "name": q["displayName"].as_str().unwrap_or(k),
                        "used": q["used"], "total": q["total"], "remainingPct": q["remainingPercentage"],
                        "unlimited": q["unlimited"], "resetAt": q["resetAt"],
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // Only the pooled limits matter (every model draws from them): 5h + weekly for Gemini and for Claude/GPT.
    const POOLS: [&str; 4] = ["gemini_session", "claude_gpt_session", "gemini_weekly", "claude_gpt_weekly"];
    let pooled: Vec<Value> = POOLS.iter().filter_map(|k| rows.iter().find(|r| r["model"] == *k).cloned()).collect();
    if !pooled.is_empty() {
        rows = pooled;
    } else {
        rows.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    }
    Ok(json!({ "plan": v["plan"], "tier": tier, "message": v["message"], "quotas": rows }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels() {
        assert_eq!(label("http://127.0.0.1:20128"), "Máy này");
        assert_eq!(label("https://160.1.2.3"), "VPS 160.1.2.3");
        assert!(valid_id("5dfe52d3-890b"));
        assert!(!valid_id("../auth"));
    }
}
