pub mod anthropic;
pub mod mock;
pub mod openai;

use crate::store::S;
use crate::types::{Employee, ProviderId};
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema object for the tool input.
    pub schema: Value,
}

#[derive(Clone, Debug)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Clone, Debug)]
pub struct ToolResult {
    pub id: String,
    pub content: String,
    pub is_error: bool,
}

#[derive(Clone, Debug)]
pub enum LlmMessage {
    User {
        text: Option<String>,
        tool_results: Vec<ToolResult>,
    },
    Assistant {
        text: String,
        tool_calls: Vec<ToolCall>,
        /// Provider-native content, replayed unchanged to the same provider (keeps thinking blocks valid).
        raw: Option<Value>,
        raw_provider: Option<ProviderId>,
    },
}

#[derive(Clone, Debug)]
pub struct TicketMeta {
    pub key: String,
    pub title: String,
    pub bounces: u32,
}

/// Context about the job; only the mock provider uses it to script its behaviour.
#[derive(Clone, Debug)]
pub struct Meta {
    pub phase: &'static str,
    pub project_id: String,
    /// Extra context for the mock (e.g. the customer conversation).
    pub extra: String,
    pub project_name: String,
    pub request: String,
    pub ticket: Option<TicketMeta>,
}

pub struct LlmRequest<'a> {
    pub employee: &'a Employee,
    pub system: &'a str,
    pub messages: &'a [LlmMessage],
    pub tools: &'a [ToolDef],
    pub max_tokens: u32,
    pub meta: &'a Meta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    End,
    Tool,
    MaxTokens,
    Refusal,
    Other,
}

#[derive(Clone, Debug)]
pub struct LlmResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub stop: Stop,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub raw: Option<Value>,
    pub raw_provider: Option<ProviderId>,
}

pub fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .connect_timeout(Duration::from_secs(20))
            .build()
            .expect("http client")
    })
}

pub async fn chat(store: &S, req: LlmRequest<'_>, cancel: &CancellationToken) -> Result<LlmResponse, String> {
    let fut = async {
        match req.employee.provider {
            ProviderId::Mock => mock::chat(store, &req).await,
            ProviderId::Anthropic => anthropic::chat(&req).await,
            _ => openai::chat(&req).await,
        }
    };
    tokio::select! {
        r = fut => r,
        _ = cancel.cancelled() => Err("aborted".into()),
    }
}

/// Retry-After (seconds) or exponential backoff.
pub fn backoff(attempt: u32, retry_after: Option<&reqwest::header::HeaderValue>) -> Duration {
    retry_after
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|s| Duration::from_secs(s.min(60)))
        .unwrap_or_else(|| Duration::from_millis(1500 * 2u64.pow(attempt)))
}
