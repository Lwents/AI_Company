//! Claude Messages API over raw HTTP (Rust has no official Anthropic SDK).
use super::{backoff, http, LlmMessage, LlmRequest, LlmResponse, Stop, ToolCall};
use crate::types::ProviderId;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};

const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

/// Turned off for the rest of the process if the API rejects the fallback parameter.
static FALLBACKS_ENABLED: AtomicBool = AtomicBool::new(true);

/// Models that accept the server-side refusal fallback (`fallbacks: "default"`).
fn supports_fallback(model: &str) -> bool {
    matches!(
        model,
        "claude-fable-5-2" | "claude-fable-5-1" | "claude-opus-5-5" | "claude-opus-5" | "claude-sonnet-5-5"
    )
}

/// Models that accept `output_config.effort`.
fn supports_effort(model: &str) -> bool {
    ["claude-fable", "claude-mythos", "claude-opus-5", "claude-opus-4-8", "claude-opus-4-7", "claude-opus-4-6", "claude-opus-4-5", "claude-sonnet-5", "claude-sonnet-4-6", "claude-haiku-5"]
        .iter()
        .any(|p| model.starts_with(p))
}

fn to_messages(messages: &[LlmMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|m| match m {
            LlmMessage::Assistant { text, tool_calls, raw, raw_provider } => {
                if let (Some(raw), Some(ProviderId::Anthropic)) = (raw, raw_provider) {
                    // Replay content unchanged (thinking blocks must not be edited).
                    return json!({ "role": "assistant", "content": raw });
                }
                let mut content = Vec::new();
                if !text.is_empty() {
                    content.push(json!({ "type": "text", "text": text }));
                }
                for c in tool_calls {
                    content.push(json!({ "type": "tool_use", "id": c.id, "name": c.name, "input": c.input }));
                }
                json!({ "role": "assistant", "content": content })
            }
            LlmMessage::User { text, tool_results } => {
                let mut content = Vec::new();
                for r in tool_results {
                    content.push(json!({ "type": "tool_result", "tool_use_id": r.id, "content": r.content, "is_error": r.is_error }));
                }
                if let Some(t) = text {
                    content.push(json!({ "type": "text", "text": t }));
                }
                json!({ "role": "user", "content": content })
            }
        })
        .collect()
}

pub async fn chat(req: &LlmRequest<'_>) -> Result<LlmResponse, String> {
    let e = req.employee;
    let base = std::env::var("ANTHROPIC_BASE_URL").unwrap_or_else(|_| "https://api.anthropic.com".into());
    let url = format!("{}/v1/messages", base.trim_end_matches('/'));
    let api_key = std::env::var("ANTHROPIC_API_KEY").ok().filter(|s| !s.is_empty());
    let auth_token = std::env::var("ANTHROPIC_AUTH_TOKEN").ok().filter(|s| !s.is_empty());
    if api_key.is_none() && auth_token.is_none() {
        return Err("Thiếu ANTHROPIC_API_KEY trong file .env".into());
    }

    let tools: Vec<Value> = req
        .tools
        .iter()
        .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.schema }))
        .collect();
    let mut body = json!({
        "model": e.model,
        "max_tokens": req.max_tokens,
        "system": req.system,
        "tools": tools,
        "messages": to_messages(req.messages),
    });
    if !e.effort.is_empty() && supports_effort(&e.model) {
        body["output_config"] = json!({ "effort": e.effort });
    }

    let mut attempt = 0;
    let data: Value = loop {
        let use_fallback = supports_fallback(&e.model) && FALLBACKS_ENABLED.load(Ordering::Relaxed);
        let mut b = body.clone();
        let mut rb = http()
            .post(&url)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json");
        rb = match (&api_key, &auth_token) {
            (Some(k), _) => rb.header("x-api-key", k),
            (None, Some(t)) => rb.bearer_auth(t),
            _ => rb,
        };
        if use_fallback {
            // On a policy decline the API reruns the turn on a fallback model inside the same call.
            b["fallbacks"] = json!("default");
            rb = rb.header("anthropic-beta", FALLBACK_BETA);
        }
        let res = rb.json(&b).send().await.map_err(|err| format!("Anthropic: {err}"))?;
        let status = res.status();
        if status.is_success() {
            break res.json().await.map_err(|err| format!("Anthropic: {err}"))?;
        }
        let retry_after = res.headers().get("retry-after").cloned();
        let text = res.text().await.unwrap_or_default();
        if status.as_u16() == 400 && use_fallback && text.to_lowercase().contains("fallback") {
            FALLBACKS_ENABLED.store(false, Ordering::Relaxed);
            continue;
        }
        let retryable = status.as_u16() == 429 || status.as_u16() == 529 || status.is_server_error();
        if retryable && attempt < 4 {
            tokio::time::sleep(backoff(attempt, retry_after.as_ref())).await;
            attempt += 1;
            continue;
        }
        return Err(format!("Anthropic {}: {}", status.as_u16(), text.chars().take(600).collect::<String>()));
    };

    let content = data["content"].as_array().cloned().unwrap_or_default();
    let text = content
        .iter()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let tool_calls = content
        .iter()
        .filter(|b| b["type"] == "tool_use")
        .map(|b| ToolCall {
            id: b["id"].as_str().unwrap_or_default().to_string(),
            name: b["name"].as_str().unwrap_or_default().to_string(),
            input: b["input"].clone(),
        })
        .collect::<Vec<_>>();
    let stop = match data["stop_reason"].as_str() {
        Some("tool_use") => Stop::Tool,
        Some("end_turn") => Stop::End,
        Some("max_tokens") => Stop::MaxTokens,
        Some("refusal") => Stop::Refusal,
        _ => Stop::Other,
    };
    Ok(LlmResponse {
        text,
        tool_calls,
        stop,
        input_tokens: data["usage"]["input_tokens"].as_u64().unwrap_or(0)
            + data["usage"]["cache_read_input_tokens"].as_u64().unwrap_or(0)
            + data["usage"]["cache_creation_input_tokens"].as_u64().unwrap_or(0),
        output_tokens: data["usage"]["output_tokens"].as_u64().unwrap_or(0),
        raw: Some(Value::Array(content)),
        raw_provider: Some(ProviderId::Anthropic),
    })
}
