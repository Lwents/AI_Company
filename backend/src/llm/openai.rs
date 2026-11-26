//! OpenAI-compatible Chat Completions: OpenAI, OpenRouter, Gemini, Ollama and custom endpoints.
use super::{backoff, http, LlmMessage, LlmRequest, LlmResponse, Stop, ToolCall};
use crate::config;
use crate::types::ProviderId;
use serde_json::{json, Value};

fn to_messages(system: &str, messages: &[LlmMessage]) -> Vec<Value> {
    let mut out = vec![json!({ "role": "system", "content": system })];
    // Index of the first image still sent in full (older ones are replaced by a note).
    let total: usize = messages
        .iter()
        .map(|m| match m {
            LlmMessage::User { tool_results, .. } => tool_results.iter().map(|r| r.images.len()).sum(),
            _ => 0,
        })
        .sum();
    let keep_from = total.saturating_sub(super::KEEP_IMAGES);
    let mut seen = 0usize;
    for m in messages {
        match m {
            LlmMessage::Assistant { text, tool_calls, .. } => {
                let mut msg = json!({ "role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text) } });
                if !tool_calls.is_empty() {
                    msg["tool_calls"] = tool_calls
                        .iter()
                        .map(|c| {
                            json!({ "id": c.id, "type": "function",
                                    "function": { "name": c.name, "arguments": c.input.to_string() } })
                        })
                        .collect();
                }
                out.push(msg);
            }
            LlmMessage::User { text, tool_results } => {
                let mut parts: Vec<Value> = vec![];
                for r in tool_results {
                    let content = if r.is_error { format!("LỖI: {}", r.content) } else { r.content.clone() };
                    out.push(json!({ "role": "tool", "tool_call_id": r.id, "content": content }));
                    for (mime, data) in &r.images {
                        if seen >= keep_from {
                            parts.push(json!({ "type": "image_url", "image_url": { "url": format!("data:{mime};base64,{data}") } }));
                        }
                        seen += 1;
                    }
                }
                // Chat Completions can't put images in tool messages: attach them as a user turn.
                if !parts.is_empty() {
                    parts.insert(0, json!({ "type": "text", "text": "Ảnh chụp màn hình từ công cụ ở trên:" }));
                    out.push(json!({ "role": "user", "content": parts }));
                }
                if let Some(t) = text {
                    out.push(json!({ "role": "user", "content": t }));
                }
            }
        }
    }
    out
}

/// Tries per endpoint for network errors, 429 and 5xx.
const MAX_ATTEMPTS: u32 = 6;

pub async fn chat(req: &LlmRequest<'_>) -> Result<LlmResponse, String> {
    let e = req.employee;
    let base = if e.provider == ProviderId::Custom {
        Some(e.base_url.clone()).filter(|s| !s.trim().is_empty())
    } else {
        config::provider_base_url(e.provider)
    }
    .ok_or_else(|| format!("Chưa cấu hình Base URL cho {}", e.name))?;
    let key = config::provider_env_key(e.provider).and_then(|k| std::env::var(k).ok()).filter(|s| !s.is_empty());
    if key.is_none() && !matches!(e.provider, ProviderId::Ollama | ProviderId::Custom) {
        return Err(format!("Thiếu {} trong file .env", config::provider_env_key(e.provider).unwrap_or("API key")));
    }

    let mut body = json!({
        "model": e.model,
        "stream": false,
        "max_tokens": req.max_tokens,
        "messages": to_messages(req.system, req.messages),
        "tools": req.tools.iter().map(|t| json!({
            "type": "function",
            "function": { "name": t.name, "description": t.description, "parameters": t.schema }
        })).collect::<Vec<_>>(),
    });
    if req.tools.is_empty() {
        if let Some(o) = body.as_object_mut() {
            o.remove("tools");
        }
    }
    // Backup endpoint (e.g. a local 9router) used when the primary one keeps failing.
    let fallback = (e.provider == ProviderId::Custom)
        .then(|| std::env::var("CUSTOM_FALLBACK_BASE_URL").ok())
        .flatten()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty() && s != base.trim_end_matches('/'));
    let mut bases = vec![base.trim_end_matches('/').to_string()];
    bases.extend(fallback);
    let mut last_err = String::new();
    let mut data: Option<Value> = None;
    'endpoints: for (bi, b) in bases.iter().enumerate() {
        let url = format!("{b}/chat/completions");
        for attempt in 0..MAX_ATTEMPTS {
            let mut rb = http().post(&url).json(&body);
            if let Some(k) = &key {
                rb = rb.bearer_auth(k);
            }
            let wait = match rb.send().await {
                // Network error / timeout: worth retrying.
                Err(err) => {
                    last_err = format!("{:?}: {err}", e.provider);
                    backoff(attempt, None)
                }
                Ok(res) => {
                    let status = res.status();
                    if status.is_success() {
                        match res.json::<Value>().await {
                            Ok(v) => {
                                data = Some(v);
                                break 'endpoints;
                            }
                            Err(err) => {
                                last_err = format!("{:?}: {err}", e.provider);
                                backoff(attempt, None)
                            }
                        }
                    } else {
                        let retry_after = res.headers().get("retry-after").cloned();
                        let text = res.text().await.unwrap_or_default();
                        last_err = format!("{:?} {}: {}", e.provider, status.as_u16(), text.chars().take(600).collect::<String>());
                        if !(status.as_u16() == 429 || status.is_server_error()) {
                            // 4xx: the request itself is the problem; another endpoint won't help either.
                            return Err(last_err);
                        }
                        // 9router says how long an account is cooling down ("reset after 1m 19s").
                        super::reset_after(&text).unwrap_or_else(|| backoff(attempt, retry_after.as_ref()))
                    }
                }
            };
            if attempt + 1 < MAX_ATTEMPTS {
                tokio::time::sleep(wait).await;
            }
        }
        if bi + 1 < bases.len() {
            eprintln!("Endpoint {b} lỗi liên tục, chuyển sang endpoint dự phòng");
        }
    }
    let Some(data) = data else { return Err(last_err) };

    let choice = &data["choices"][0];
    let msg = &choice["message"];
    let tool_calls: Vec<ToolCall> = msg["tool_calls"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .enumerate()
                .map(|(i, c)| {
                    let args = c["function"]["arguments"].as_str().unwrap_or("{}");
                    let input = serde_json::from_str(args).unwrap_or_else(|_| json!({ "__invalid_json": args }));
                    ToolCall {
                        id: c["id"].as_str().map(str::to_string).unwrap_or_else(|| format!("call_{i}")),
                        name: c["function"]["name"].as_str().unwrap_or_default().to_string(),
                        input,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let stop = if !tool_calls.is_empty() {
        Stop::Tool
    } else {
        match choice["finish_reason"].as_str() {
            Some("length") => Stop::MaxTokens,
            Some("content_filter") => Stop::Refusal,
            _ => Stop::End,
        }
    };
    Ok(LlmResponse {
        text: msg["content"].as_str().unwrap_or_default().to_string(),
        tool_calls,
        stop,
        input_tokens: data["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
        output_tokens: {
            // Some proxies report thinking tokens separately instead of inside completion_tokens;
            // they are billed as output, so count them.
            let completion = data["usage"]["completion_tokens"].as_u64().unwrap_or(0);
            let reasoning = data["usage"]["completion_tokens_details"]["reasoning_tokens"].as_u64().unwrap_or(0);
            if reasoning > completion { completion + reasoning } else { completion }
        },
        raw: None,
        raw_provider: None,
    })
}
