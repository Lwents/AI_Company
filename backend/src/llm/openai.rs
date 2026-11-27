//! OpenAI-compatible Chat Completions: OpenAI, OpenRouter, Gemini, Ollama and custom endpoints.
use super::{backoff, http, LlmMessage, LlmRequest, LlmResponse, Stop, ToolCall};
use crate::config;
use crate::types::ProviderId;
use serde_json::{json, Value};

fn to_messages(system: &str, messages: &[LlmMessage]) -> Vec<Value> {
    let mut out = vec![json!({ "role": "system", "content": system })];
    // Index of the first image still sent in full (older ones are replaced by a note).
    let total_images: usize = messages
        .iter()
        .map(|m| match m {
            LlmMessage::User { tool_results, .. } => tool_results.iter().map(|r| r.images.len()).sum(),
            _ => 0,
        })
        .sum();
    let keep_from = total_images.saturating_sub(super::KEEP_IMAGES);
    let total_tool_turns = messages
        .iter()
        .filter(|m| matches!(m, LlmMessage::User { tool_results, .. } if !tool_results.is_empty()))
        .count();
    let keep_full_from = total_tool_turns.saturating_sub(super::KEEP_FULL_TOOL_TURNS);
    let mut seen = 0usize;
    let mut tool_turn = 0usize;
    for m in messages {
        match m {
            LlmMessage::Assistant { text, tool_calls, .. } => {
                // Gemini/OpenAI proxies reject assistant messages with null content and no tool_calls (400 INVALID_ARGUMENT).
                let content = if text.is_empty() {
                    if tool_calls.is_empty() { json!("(trống)") } else { Value::Null }
                } else {
                    json!(text)
                };
                let mut msg = json!({ "role": "assistant", "content": content });
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
                let keep_full = tool_turn >= keep_full_from;
                if !tool_results.is_empty() {
                    tool_turn += 1;
                }
                let mut parts: Vec<Value> = vec![];
                for r in tool_results {
                    let body = super::compact_tool_result(&r.content, keep_full);
                    let content = if r.is_error {
                        format!("LỖI: {body}")
                    } else if body.is_empty() {
                        "(trống)".to_string()
                    } else {
                        body
                    };
                    out.push(json!({ "role": "tool", "tool_call_id": r.id, "content": content }));
                    for (mime, data) in &r.images {
                        if seen >= keep_from {
                            parts.push(json!({ "type": "image_url", "image_url": { "url": format!("data:{mime};base64,{data}") } }));
                        }
                        seen += 1;
                    }
                }
                // Chat Completions can't put images in tool messages: attach them (and optional text) in a single user turn.
                if !parts.is_empty() {
                    parts.insert(0, json!({ "type": "text", "text": "Ảnh chụp màn hình từ công cụ ở trên:" }));
                    if let Some(t) = text.as_deref().filter(|s| !s.is_empty()) {
                        parts.push(json!({ "type": "text", "text": t }));
                    }
                    out.push(json!({ "role": "user", "content": parts }));
                } else if let Some(t) = text.as_deref().filter(|s| !s.is_empty()) {
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
        let gate = format!("{b}|{}", e.model);
        for attempt in 0..MAX_ATTEMPTS {
            // Another employee just hit a rate limit on this endpoint+model: wait with them.
            super::gate_wait(&gate).await;
            let mut rb = http().post(&url).json(&body);
            // Custom endpoints may each have their own key (CUSTOM_API_KEYS), else CUSTOM_API_KEY.
            let key = if e.provider == ProviderId::Custom { config::custom_key_for(b).or_else(|| key.clone()) } else { key.clone() };
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
                        let w = super::reset_after(&text).unwrap_or_else(|| backoff(attempt, retry_after.as_ref()));
                        if status.as_u16() == 429 {
                            super::gate_block(&gate, w);
                        }
                        w
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{LlmMessage, ToolResult};

    #[test]
    fn avoids_invalid_argument_shapes_and_compacts_old_results() {
        let big = "x".repeat(7000);
        let mut msgs = vec![
            LlmMessage::User { text: Some("start".into()), tool_results: vec![] },
            LlmMessage::Assistant { text: String::new(), tool_calls: vec![], raw: None, raw_provider: None },
        ];
        // Add KEEP_FULL_TOOL_TURNS + 2 tool turns so the first 2 are compacted while the rest stay full.
        let total = crate::llm::KEEP_FULL_TOOL_TURNS + 2;
        for i in 0..total {
            msgs.push(LlmMessage::User {
                text: None,
                tool_results: vec![ToolResult { id: format!("c{i}"), content: big.clone(), is_error: false, images: vec![] }],
            });
        }
        let out = to_messages("sys", &msgs);
        // Empty assistant turn with no tool_calls must not have null content.
        assert_eq!(out[2]["role"], "assistant");
        assert_eq!(out[2]["content"], "(trống)");
        // Old tool result is compacted; latest tool result is preserved in full.
        assert!(out[3]["content"].as_str().unwrap().contains("đã rút gọn"));
        assert_eq!(out[2 + total]["content"].as_str().unwrap().len(), 7000);
    }
}
