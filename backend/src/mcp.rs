//! Minimal MCP (Model Context Protocol) client over stdio.
//!
//! Servers are configured in `mcp.json` at the company root (same shape as Claude's config, plus
//! `roles`: which employees get the server). Each job starts the servers its role needs in the
//! project folder and stops them when the job ends, so browsers etc. never leak between jobs.
use crate::types::Role;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};

#[derive(Deserialize, Clone, Debug)]
pub struct ServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Roles that get this server (snake_case, e.g. "tester"); empty = everyone.
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Shown to the model, explains when to use these tools.
    #[serde(default)]
    pub hint: String,
    /// Tools of this server never offered to employees.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Optional allowlist: when non-empty, ONLY these tool names are exposed to employees.
    #[serde(default)]
    pub include: Vec<String>,
    /// Operating systems this server runs on ("windows", "linux", "macos"); empty = all.
    #[serde(default)]
    pub os: Vec<String>,
    /// Replacement `args` on Linux (e.g. chromium instead of Edge).
    #[serde(default, rename = "argsLinux")]
    pub args_linux: Option<Vec<String>>,
}

impl ServerConfig {
    fn for_this_os(&self) -> bool {
        self.os.is_empty() || self.os.iter().any(|o| o == std::env::consts::OS)
    }
    fn effective_args(&self) -> &[String] {
        match (&self.args_linux, std::env::consts::OS) {
            (Some(a), "linux") => a,
            _ => &self.args,
        }
    }
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, Default)]
struct ConfigFile {
    #[serde(default, rename = "mcpServers")]
    servers: HashMap<String, ServerConfig>,
}

pub fn load_config(root: &Path) -> HashMap<String, ServerConfig> {
    std::fs::read_to_string(root.join("mcp.json"))
        .ok()
        .and_then(|s| match serde_json::from_str::<ConfigFile>(&s) {
            Ok(c) => Some(c.servers),
            Err(e) => {
                eprintln!("mcp.json không hợp lệ: {e}");
                None
            }
        })
        .unwrap_or_default()
}

pub fn role_key(r: Role) -> String {
    serde_json::to_value(r).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

/// A tool exposed by a server, as seen by the model.
#[derive(Clone, Debug)]
pub struct McpTool {
    /// Name given to the model: `<server>__<tool>` (letters, digits, underscore).
    pub name: String,
    pub server: String,
    pub tool: String,
    pub description: String,
    pub schema: Value,
}

/// Result of a tool call: text plus images (base64, mime).
pub struct CallResult {
    pub text: String,
    pub images: Vec<(String, String)>,
    pub is_error: bool,
}

struct Conn {
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    next_id: u64,
}

impl Conn {
    async fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.stdin.write_all(format!("{msg}\n").as_bytes()).await.map_err(|e| e.to_string())?;
        self.stdin.flush().await.map_err(|e| e.to_string())?;
        let read = async {
            loop {
                let Some(line) = self.lines.next_line().await.map_err(|e| e.to_string())? else {
                    return Err("MCP server đã thoát".to_string());
                };
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                if v.get("id").and_then(Value::as_u64) == Some(id) {
                    if let Some(err) = v.get("error") {
                        return Err(err.get("message").and_then(Value::as_str).unwrap_or("lỗi MCP").to_string());
                    }
                    return Ok(v.get("result").cloned().unwrap_or(Value::Null));
                }
                // Server-initiated requests (roots/list, ping…): answer with an empty result.
                if let (Some(rid), Some(_)) = (v.get("id"), v.get("method")) {
                    let reply = json!({ "jsonrpc": "2.0", "id": rid, "result": {} });
                    let _ = self.stdin.write_all(format!("{reply}\n").as_bytes()).await;
                }
            }
        };
        tokio::time::timeout(timeout, read).await.map_err(|_| format!("MCP {method} quá thời gian"))?
    }

    async fn notify(&mut self, method: &str) {
        let msg = json!({ "jsonrpc": "2.0", "method": method });
        let _ = self.stdin.write_all(format!("{msg}\n").as_bytes()).await;
        let _ = self.stdin.flush().await;
    }
}

/// The MCP servers running for one job.
pub struct Session {
    conns: HashMap<String, Conn>,
    pub tools: Vec<McpTool>,
    /// Usage notes per server, added to the system prompt.
    pub hints: Vec<String>,
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect()
}

/// Strips JSON-Schema keywords some providers (e.g. Gemini) reject with 400 INVALID_ARGUMENT.
fn clean_schema(mut v: Value) -> Value {
    if let Some(o) = v.as_object_mut() {
        for k in ["$schema", "additionalProperties", "exclusiveMinimum", "exclusiveMaximum", "const", "propertyNames", "$defs", "definitions"] {
            o.remove(k);
        }
        if let Some(Value::Array(arr)) = o.get("type").cloned() {
            if let Some(first) = arr.iter().filter_map(Value::as_str).find(|t| *t != "null") {
                o.insert("type".into(), Value::String(first.to_string()));
            }
        }
        for (_, child) in o.iter_mut() {
            *child = clean_schema(child.take());
        }
    } else if let Some(a) = v.as_array_mut() {
        for child in a.iter_mut() {
            *child = clean_schema(child.take());
        }
    }
    v
}

/// Light / coordination phases that never need external MCP servers (saves spawning npx/browser per turn).
fn skip_mcp_for_phase(phase: &str) -> bool {
    matches!(
        phase,
        "meeting_opinion" | "meeting_ballot" | "meeting_vote" | "minutes" | "standup" | "planning" | "clarify" | "retro"
    )
}

/// Phases where Playwright browser automation is actually useful.
fn browser_phase(phase: &str) -> bool {
    matches!(
        phase,
        "design" | "implement" | "review" | "test" | "design_review" | "final_qa" | "security" | "documentation" | "release"
    )
}

/// Phases where native Windows desktop GUI automation (`desktop` MCP) is offered (QA / visual testing).
fn desktop_phase(phase: &str) -> bool {
    matches!(phase, "test" | "final_qa" | "design_review" | "release")
}

impl Session {
    /// Starts every enabled server meant for `role` and `phase` in parallel. Servers that fail to start are skipped.
    pub async fn start(root: &Path, dir: &Path, role: Role, phase: &str) -> Session {
        let mut s = Session { conns: HashMap::new(), tools: vec![], hints: vec![] };
        if skip_mcp_for_phase(phase) {
            return s;
        }
        let rk = role_key(role);
        let mut servers: Vec<(String, ServerConfig)> = load_config(root)
            .into_iter()
            .filter(|(name, c)| {
                c.enabled
                    && c.for_this_os()
                    && (c.roles.is_empty() || c.roles.iter().any(|r| r == &rk))
                    && (name != "browser" || browser_phase(phase))
                    && (name != "desktop" || desktop_phase(phase))
            })
            .collect();
        servers.sort_by(|a, b| a.0.cmp(&b.0));
        let mut set = tokio::task::JoinSet::new();
        for (name, cfg) in servers {
            let root = root.to_path_buf();
            let dir = dir.to_path_buf();
            set.spawn(async move {
                let res = start_server(&root, &dir, &cfg).await;
                (name, cfg, res)
            });
        }
        let mut started = vec![];
        while let Some(joined) = set.join_next().await {
            if let Ok((name, cfg, res)) = joined {
                match res {
                    Ok((conn, tools)) => started.push((name, cfg, conn, tools)),
                    Err(e) => eprintln!("Không khởi động được MCP {name}: {e}"),
                }
            }
        }
        started.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, cfg, conn, tools) in started {
            for t in tools {
                let tool = t.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                if tool.is_empty() || cfg.exclude.iter().any(|x| x == &tool) {
                    continue;
                }
                if !cfg.include.is_empty() && !cfg.include.iter().any(|x| x == &tool) {
                    continue;
                }
                let full: String = format!("{}__{}", sanitize(&name), sanitize(&tool)).chars().take(64).collect();
                s.tools.push(McpTool {
                    name: full,
                    server: name.clone(),
                    tool,
                    description: t.get("description").and_then(Value::as_str).unwrap_or_default().chars().take(1000).collect(),
                    schema: clean_schema(t.get("inputSchema").cloned().unwrap_or(json!({ "type": "object", "properties": {} }))),
                });
            }
            if !cfg.hint.is_empty() {
                s.hints.push(format!("- {name}: {}", cfg.hint));
            }
            s.conns.insert(name, conn);
        }
        s
    }

    pub fn has(&self, model_name: &str) -> bool {
        self.tools.iter().any(|t| t.name == model_name)
    }

    pub async fn call(&mut self, model_name: &str, args: &Value) -> CallResult {
        let Some(t) = self.tools.iter().find(|t| t.name == model_name).cloned() else {
            return CallResult { text: format!("Không có công cụ {model_name}"), images: vec![], is_error: true };
        };
        let Some(conn) = self.conns.get_mut(&t.server) else {
            return CallResult { text: format!("MCP {} không chạy", t.server), images: vec![], is_error: true };
        };
        let args = if args.is_object() { args.clone() } else { json!({}) };
        match conn.request("tools/call", json!({ "name": t.tool, "arguments": args }), Duration::from_secs(180)).await {
            Ok(r) => {
                let mut text = String::new();
                let mut images = vec![];
                for c in r.get("content").and_then(Value::as_array).cloned().unwrap_or_default() {
                    match c.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(c.get("text").and_then(Value::as_str).unwrap_or_default());
                        }
                        Some("image") => {
                            if let (Some(d), Some(m)) = (c.get("data").and_then(Value::as_str), c.get("mimeType").and_then(Value::as_str)) {
                                images.push((m.to_string(), d.to_string()));
                            }
                        }
                        _ => {}
                    }
                }
                // Page snapshots can be huge; keep the start (enough to act on).
                if text.chars().count() > 30_000 {
                    text = text.chars().take(30_000).collect::<String>() + "\n…(đã cắt bớt)";
                }
                if text.is_empty() && !images.is_empty() {
                    text = "(ảnh đính kèm)".into();
                }
                CallResult { text, images, is_error: r.get("isError").and_then(Value::as_bool).unwrap_or(false) }
            }
            Err(e) => CallResult { text: e, images: vec![], is_error: true },
        }
    }

}

impl Drop for Session {
    /// Stops the servers and everything they started (npx → node → browser).
    fn drop(&mut self) {
        for (_, c) in self.conns.iter_mut() {
            if let Some(pid) = c.child.id() {
                if cfg!(windows) {
                    let _ = std::process::Command::new("taskkill").args(["/T", "/F", "/PID", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null()).status();
                } else {
                    let _ = std::process::Command::new("kill").args(["-KILL", &format!("-{pid}")]).stdout(Stdio::null()).stderr(Stdio::null()).status();
                }
            }
            let _ = c.child.start_kill();
        }
    }
}

async fn start_server(root: &Path, dir: &Path, cfg: &ServerConfig) -> Result<(Conn, Vec<Value>), String> {
    // On Windows `.cmd`/`.bat`/PATH shims like `npx` go through cmd; direct `.exe` binaries run directly so kill_on_drop works cleanly.
    let direct_exe = cfg!(windows) && cfg.command.to_lowercase().ends_with(".exe");
    let mut cmd = if cfg!(windows) && !direct_exe {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(&cfg.command).args(cfg.effective_args());
        c
    } else {
        let mut c = tokio::process::Command::new(&cfg.command);
        c.args(cfg.effective_args());
        c
    };
    // Own process group, so the whole tree (npx → node → browser) can be stopped together.
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.current_dir(dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    for k in ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "OPENAI_API_KEY", "OPENROUTER_API_KEY", "GEMINI_API_KEY", "CUSTOM_API_KEY"] {
        cmd.env_remove(k);
    }
    let root_s = root.display().to_string();
    for (k, v) in &cfg.env {
        cmd.env(k, v.replace("{root}", &root_s));
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let stdin = child.stdin.take().ok_or("không có stdin")?;
    let stdout = child.stdout.take().ok_or("không có stdout")?;
    let mut conn = Conn { child, stdin, lines: BufReader::new(stdout).lines(), next_id: 0 };
    conn.request(
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "ai-company", "version": "0.1" } }),
        Duration::from_secs(120),
    )
    .await?;
    conn.notify("notifications/initialized").await;
    let list = conn.request("tools/list", json!({}), Duration::from_secs(60)).await?;
    let tools = list.get("tools").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok((conn, tools))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_gemini_incompatible_schema_and_skips_light_phases() {
        let s = clean_schema(json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "x": { "type": ["string", "null"], "const": "a", "exclusiveMinimum": 0 }
            }
        }));
        assert!(s.get("$schema").is_none());
        assert!(s.get("additionalProperties").is_none());
        assert_eq!(s["properties"]["x"]["type"], "string");
        assert!(s["properties"]["x"].get("const").is_none());
        assert!(skip_mcp_for_phase("meeting_opinion"));
        assert!(skip_mcp_for_phase("meeting_vote"));
        assert!(!skip_mcp_for_phase("implement"));
        assert!(!browser_phase("architecture"));
        assert!(browser_phase("test"));
        assert!(desktop_phase("test"));
        assert!(desktop_phase("final_qa"));
        assert!(!desktop_phase("implement"));
    }

    /// Needs network + Edge: `cargo test -- --ignored mcp_live`.
    #[tokio::test]
    #[ignore]
    async fn mcp_live() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let dir = std::env::temp_dir();
        let mut s = Session::start(&root, &dir, Role::Tester, "test").await;
        let mut names: Vec<_> = s.tools.iter().map(|t| t.name.clone()).collect();
        names.sort();
        println!("{} tools: {:?}", names.len(), names);
        assert!(s.has("browser__browser_navigate"), "playwright tools missing");
        let r = s.call("browser__browser_navigate", &json!({ "url": "data:text/html,<h1>Xin chao</h1><button>Bam</button>" })).await;
        println!("navigate err={} {}", r.is_error, r.text.chars().take(300).collect::<String>());
        let r = s.call("browser__browser_take_screenshot", &json!({})).await;
        println!("screenshot err={} images={} {}", r.is_error, r.images.len(), r.text.chars().take(200).collect::<String>());
        assert!(!r.images.is_empty());
        let r = s.call("browser__browser_take_screenshot", &json!({ "filename": "docs/images/mcp-test.png" })).await;
        println!("named screenshot err={} {}", r.is_error, r.text.chars().take(200).collect::<String>());
        assert!(dir.join("docs/images/mcp-test.png").is_file(), "named screenshot not in the project folder");
        let _ = std::fs::remove_dir_all(dir.join("docs").join("images"));
        let r = s.call("docs__resolve-library-id", &json!({ "libraryName": "vite", "query": "vite config base" })).await;
        println!("docs err={} {}", r.is_error, r.text.chars().take(200).collect::<String>());
    }
}
