//! OpenAI-compatible Chat Completions: OpenAI, OpenRouter, Gemini, Ollama and custom endpoints.
use super::{backoff, http, LlmMessage, LlmRequest, LlmResponse, Stop, ToolCall};
use crate::config;
use crate::types::ProviderId;
use serde_json::{json, Value};

fn to_messages(system: &str, messages: &[LlmMessage]) -> Vec<Value> {
    let mut out = vec![json!({ "role": "system", "content": system })];
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
                for r in tool_results {
                    let content = if r.is_error { format!("LỖI: {}", r.content) } else { r.content.clone() };
                    out.push(json!({ "role": "tool", "tool_call_id": r.id, "content": content }));
                }
                if let Some(t) = text {
                    out.push(json!({ "role": "user", "content": t }));
                }
            }
        }
    }
    out
}

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

    let body = json!({
        "model": e.model,
        "stream": false,
        "max_tokens": req.max_tokens,
        "messages": to_messages(req.system, req.messages),
        "tools": req.tools.iter().map(|t| json!({
            "type": "function",
            "function": { "name": t.name, "description": t.description, "parameters": t.schema }
        })).collect::<Vec<_>>(),
    });
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let mut attempt = 0;
    let data: Value = loop {
        let mut rb = http().post(&url).json(&body);
        if let Some(k) = &key {
            rb = rb.bearer_auth(k);
        }
        let res = rb.send().await.map_err(|err| format!("{:?}: {err}", e.provider))?;
        let status = res.status();
        if status.is_success() {
            break res.json().await.map_err(|err| format!("{:?}: {err}", e.provider))?;
        }
        let retry_after = res.headers().get("retry-after").cloned();
        let text = res.text().await.unwrap_or_default();
        if (status.as_u16() == 429 || status.is_server_error()) && attempt < 4 {
            tokio::time::sleep(backoff(attempt, retry_after.as_ref())).await;
            attempt += 1;
            continue;
        }
        return Err(format!("{:?} {}: {}", e.provider, status.as_u16(), text.chars().take(600).collect::<String>()));
    };

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
        output_tokens: data["usage"]["completion_tokens"].as_u64().unwrap_or(0),
        raw: None,
        raw_provider: None,
    })
}
