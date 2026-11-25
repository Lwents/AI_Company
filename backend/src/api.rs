//! HTTP + WebSocket API for the office UI.
use crate::store::{new_id, now_ms, S};
use crate::tools::{list_dir, safe_path};
use crate::types::*;
use crate::ops;
use crate::workflow;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

type ApiResult = Result<Json<Value>, (StatusCode, String)>;

fn bad(msg: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, msg.into())
}

pub fn router(store: S) -> Router {
    Router::new()
        .route("/api/state", get(get_state))
        .route("/ws", get(ws))
        .route("/api/projects", post(create_project))
        .route("/api/projects/{id}", axum::routing::delete(delete_project))
        .route("/api/projects/{id}/files", get(project_files))
        .route("/api/projects/{id}/file", get(project_file))
        .route("/api/projects/{id}/{action}", post(project_action))
        .route("/api/approvals/{id}", post(resolve_approval))
        .route("/api/employees", post(create_employee))
        .route("/api/employees/{id}", put(update_employee).delete(delete_employee))
        .route("/api/settings", put(update_settings))
        .route("/api/chat", post(chairman_chat))
        .route("/preview/{id}/{*path}", get(preview))
        .route("/preview/{id}/", get(preview_index))
        .route("/api/cases/{id}/reply", post(case_reply))
        .route("/api/cases/{id}/resolve", post(case_resolve))
        .merge(crate::portal::routes())
        .with_state(store)
}

/// Routes safe to expose to customers on a public address: the portal and the product itself.
pub fn public_router(store: S) -> Router {
    Router::new()
        .route("/preview/{id}/{*path}", get(public_preview))
        .route("/preview/{id}/", get(preview_index))
        .merge(crate::portal::routes())
        .with_state(store)
}

#[derive(Deserialize)]
struct CaseReply {
    text: String,
}

/// The chairman answers a customer directly.
async fn case_reply(State(store): State<S>, Path(id): Path<String>, Json(b): Json<CaseReply>) -> ApiResult {
    let text: String = b.text.trim().chars().take(4000).collect();
    if text.is_empty() {
        return Err(bad("Nội dung trống"));
    }
    store
        .update_case(&id, |c| {
            c.messages.push(CaseMsg { from: "chairman".into(), text, ts: now_ms() });
            if c.status != CaseStatus::Escalated {
                c.status = CaseStatus::WaitingCustomer;
            }
        })
        .ok_or_else(|| bad("Hồ sơ không tồn tại"))?;
    Ok(Json(json!({ "ok": true })))
}

async fn case_resolve(State(store): State<S>, Path(id): Path<String>) -> ApiResult {
    store
        .update_case(&id, |c| {
            c.status = CaseStatus::Resolved;
            c.messages.push(CaseMsg { from: "system".into(), text: "Ban giám đốc đã đóng hồ sơ.".into(), ts: now_ms() });
        })
        .ok_or_else(|| bad("Hồ sơ không tồn tại"))?;
    Ok(Json(json!({ "ok": true })))
}

async fn get_state(State(store): State<S>) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], store.snapshot_event()).into_response()
}

async fn ws(State(store): State<S>, up: WebSocketUpgrade) -> Response {
    up.on_upgrade(move |socket| ws_loop(store, socket))
}

async fn ws_loop(store: S, mut socket: WebSocket) {
    let mut rx = store.subscribe();
    if socket.send(Message::Text(store.snapshot_event().into())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(text) => if socket.send(Message::Text(text.into())).await.is_err() { return },
                // Too slow: resync with a full snapshot.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    if socket.send(Message::Text(store.snapshot_event().into())).await.is_err() { return }
                }
                Err(_) => return,
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                _ => {}
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewProject {
    request: String,
    #[serde(default)]
    name: String,
    budget_usd: Option<f64>,
}

fn slugify(s: &str) -> String {
    let map = |c: char| -> char {
        let c = c.to_lowercase().next().unwrap_or(c);
        const GROUPS: [(&str, char); 7] = [
            ("àáạảãâầấậẩẫăằắặẳẵ", 'a'),
            ("èéẹẻẽêềếệểễ", 'e'),
            ("ìíịỉĩ", 'i'),
            ("òóọỏõôồốộổỗơờớợởỡ", 'o'),
            ("ùúụủũưừứựửữ", 'u'),
            ("ỳýỵỷỹ", 'y'),
            ("đ", 'd'),
        ];
        GROUPS.iter().find(|(g, _)| g.contains(c)).map(|(_, r)| *r).unwrap_or(c)
    };
    let mut out = String::new();
    for c in s.chars().map(map) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 40 {
            break;
        }
    }
    out.trim_matches('-').to_string()
}

async fn create_project(State(store): State<S>, Json(body): Json<NewProject>) -> ApiResult {
    let request = body.request.trim().to_string();
    if request.is_empty() {
        return Err(bad("Hãy nhập yêu cầu dự án"));
    }
    let name = if body.name.trim().is_empty() {
        let n: String = request.lines().next().unwrap_or("").chars().take(48).collect();
        n.trim().to_string()
    } else {
        body.name.trim().to_string()
    };
    let id = new_id();
    let base = slugify(&name);
    let slug = format!("{}-{}", if base.is_empty() { "du-an" } else { &base }, &id[id.len() - 4..]);
    let budget = body.budget_usd.unwrap_or_else(|| store.with(|s| s.settings.default_budget_usd));
    let p = Project {
        id: id.clone(),
        name,
        slug,
        request,
        feedback: vec![],
        phase: Phase::Plan,
        status: ProjectStatus::Running,
        budget_usd: budget,
        spent_usd: 0.0,
        report: String::new(),
        error: String::new(),
        ticket_seq: 0,
        release_rounds: 0,
        meetings: 0,
        created_at: now_ms(),
        updated_at: now_ms(),
    };
    store.insert_project(p.clone());
    workflow::start(&store, &id);
    Ok(Json(json!(p)))
}

#[derive(Deserialize, Default)]
struct ActionBody {
    #[serde(default)]
    feedback: String,
}

async fn project_action(State(store): State<S>, Path((id, action)): Path<(String, String)>, raw: axum::body::Bytes) -> ApiResult {
    // Body is optional (only "changes" uses it); accept an empty body with or without a JSON content type.
    let body: Option<Json<ActionBody>> = serde_json::from_slice(&raw).ok().map(Json);
    let p = store.project(&id).ok_or_else(|| bad("Dự án không tồn tại"))?;
    let running = store.cancels.lock().unwrap().contains_key(&id);
    match action.as_str() {
        "pause" => workflow::halt(&store, &id, ProjectStatus::Paused),
        "stop" => workflow::halt(&store, &id, ProjectStatus::Stopped),
        "resume" => {
            if running {
                return Err(bad("Dự án đang chạy"));
            }
            match p.phase {
                // Live product: just reopen operations.
                Phase::Operating => {
                    store.update_project(&id, |p| p.status = ProjectStatus::Operating);
                }
                Phase::AwaitingApproval | Phase::Done => return Err(bad("Dự án đã xong giai đoạn làm việc")),
                _ => workflow::start(&store, &id),
            }
        }
        "approve" => {
            if p.phase != Phase::AwaitingApproval {
                return Err(bad("Dự án chưa chờ duyệt"));
            }
            store.update_project(&id, |p| p.phase = Phase::Launch);
            store.post_message(Some(&id), "chairman", "all", "Chủ tịch đã duyệt dự án. Cảm ơn cả đội! Chuẩn bị ra mắt 🎉", "chat");
            workflow::start(&store, &id);
        }
        "close" => {
            // Stop operating the product (archive).
            workflow::halt(&store, &id, ProjectStatus::Done);
            store.update_project(&id, |p| p.phase = Phase::Done);
        }
        "simulate" => {
            if p.status != ProjectStatus::Operating {
                return Err(bad("Sản phẩm chưa vận hành"));
            }
            let n = ops::simulate_customers(&store, &id);
            return Ok(Json(json!({ "created": n })));
        }
        "campaign" | "finance" | "hr" => {
            if !matches!(p.phase, Phase::Operating | Phase::Done) {
                return Err(bad("Chỉ làm được sau khi sản phẩm đã ra mắt"));
            }
            let brief = body.map(|b| b.0.feedback).unwrap_or_default();
            let msg = ops::run_business(&store, &id, &action, &brief).map_err(bad)?;
            return Ok(Json(json!({ "message": msg })));
        }
        "changes" => {
            let fb = body.map(|b| b.0.feedback).unwrap_or_default();
            if fb.trim().is_empty() {
                return Err(bad("Hãy ghi rõ cần thay đổi gì"));
            }
            if running {
                return Err(bad("Dự án đang chạy, hãy tạm dừng trước"));
            }
            // A live product keeps its customers; operations pause while the team builds the new version.
            store.stop_ops(&id);
            store.update_project(&id, |p| {
                p.feedback.push(fb.trim().to_string());
                p.phase = Phase::Plan;
                p.release_rounds = 0;
            });
            workflow::start(&store, &id);
        }
        _ => return Err(bad("Hành động không hợp lệ")),
    }
    Ok(Json(json!(store.project(&id))))
}

async fn delete_project(State(store): State<S>, Path(id): Path<String>) -> ApiResult {
    if store.cancels.lock().unwrap().contains_key(&id) {
        return Err(bad("Dừng dự án trước khi xóa"));
    }
    store.with(|s| {
        s.projects.retain(|p| p.id != id);
        s.tickets.retain(|t| t.project_id != id);
    });
    // Files on disk are kept.
    store.emit(ServerEvent::Remove { coll: "projects", id });
    Ok(Json(json!({ "ok": true })))
}

fn project_dir(store: &S, id: &str) -> Result<std::path::PathBuf, (StatusCode, String)> {
    let p = store.project(id).ok_or_else(|| bad("Dự án không tồn tại"))?;
    Ok(store.workspace_dir.join(p.slug))
}

async fn project_files(State(store): State<S>, Path(id): Path<String>) -> ApiResult {
    let dir = project_dir(&store, &id)?;
    let files: Vec<Value> = list_dir(&dir, 1000)
        .into_iter()
        .map(|line| {
            let (path, size) = line.rsplit_once(" (").unwrap_or((&line, "0 B)"));
            json!({ "path": path, "size": size.trim_end_matches(" B)").parse::<u64>().unwrap_or(0) })
        })
        .collect();
    Ok(Json(json!({ "dir": dir.display().to_string(), "files": files })))
}

#[derive(Deserialize)]
struct FileQuery {
    path: String,
}

async fn project_file(State(store): State<S>, Path(id): Path<String>, Query(q): Query<FileQuery>) -> ApiResult {
    let dir = project_dir(&store, &id)?;
    let p = safe_path(&dir, &q.path).map_err(bad)?;
    let bytes = tokio::fs::read(&p).await.map_err(|e| bad(e.to_string()))?;
    let text: String = String::from_utf8_lossy(&bytes).chars().take(200_000).collect();
    Ok(Json(json!({ "path": q.path, "content": text })))
}

fn mime(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "md" | "txt" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// Folders with internal company documents (never served on the public address).
const INTERNAL: [&str; 9] = ["docs/", "finance/", "hr/", "sales/", "support/", "legal/notes", "tests/", "design/", ".git/"];

async fn serve_preview(store: S, id: String, path: String) -> Response {
    let Ok(dir) = project_dir(&store, &id) else { return StatusCode::NOT_FOUND.into_response() };
    let path = if path.is_empty() || path.ends_with('/') { format!("{path}index.html") } else { path };
    let Ok(file) = safe_path(&dir, &path) else { return StatusCode::BAD_REQUEST.into_response() };
    match tokio::fs::read(&file).await {
        Ok(mut bytes) => {
            let live = store.project(&id).map(|p| p.phase == Phase::Operating).unwrap_or(false);
            if live && mime(&path).starts_with("text/html") {
                // Floating "support" button on every page of a live product.
                let widget = format!(
                    r#"<a href="/portal/{id}/" target="_blank" style="position:fixed;right:16px;bottom:16px;z-index:2147483647;background:#4f6bed;color:#fff;font:600 14px system-ui,sans-serif;padding:10px 14px;border-radius:999px;text-decoration:none;box-shadow:0 6px 20px #0003">💬 Hỗ trợ</a>"#
                );
                let html = String::from_utf8_lossy(&bytes).into_owned();
                let out = match html.rfind("</body>") {
                    Some(i) => format!("{}{}{}", &html[..i], widget, &html[i..]),
                    None => format!("{html}{widget}"),
                };
                bytes = out.into_bytes();
            }
            ([(header::CONTENT_TYPE, mime(&path)), (header::CACHE_CONTROL, "no-store")], bytes).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "Không tìm thấy tệp").into_response(),
    }
}

async fn public_preview(State(store): State<S>, Path((id, path)): Path<(String, String)>) -> Response {
    let lower = path.to_lowercase().replace('\\', "/");
    if INTERNAL.iter().any(|p| lower.starts_with(p)) || lower.ends_with(".md") && !lower.starts_with("legal/") {
        return (StatusCode::NOT_FOUND, "Không tìm thấy tệp").into_response();
    }
    serve_preview(store, id, path).await
}

async fn preview(State(store): State<S>, Path((id, path)): Path<(String, String)>) -> Response {
    serve_preview(store, id, path).await
}

async fn preview_index(State(store): State<S>, Path(id): Path<String>) -> Response {
    serve_preview(store, id, String::new()).await
}

#[derive(Deserialize)]
struct ApprovalBody {
    decision: String,
    answer: Option<String>,
}

async fn resolve_approval(State(store): State<S>, Path(id): Path<String>, Json(b): Json<ApprovalBody>) -> ApiResult {
    let a = store
        .resolve_approval(&id, b.decision == "approve", b.answer.filter(|s| !s.trim().is_empty()))
        .ok_or_else(|| bad("Yêu cầu không còn chờ duyệt"))?;
    Ok(Json(json!(a)))
}

#[derive(Deserialize)]
struct NewEmployee {
    role: Role,
    name: Option<String>,
}

async fn create_employee(State(store): State<S>, Json(b): Json<NewEmployee>) -> ApiResult {
    let e = store.hire(b.role, b.name);
    Ok(Json(json!(e)))
}

async fn update_employee(State(store): State<S>, Path(id): Path<String>, Json(mut e): Json<Employee>) -> ApiResult {
    if store.employee(&id).is_none() {
        return Err(bad("Nhân viên không tồn tại"));
    }
    e.id = id;
    e.name = e.name.trim().chars().take(24).collect();
    if e.name.is_empty() {
        return Err(bad("Tên không được trống"));
    }
    store.upsert_employee(e.clone());
    Ok(Json(json!(e)))
}

async fn delete_employee(State(store): State<S>, Path(id): Path<String>) -> ApiResult {
    let busy = store.with(|s| s.runtime.get(&id).map(|r| r.status != EmpStatus::Idle).unwrap_or(false));
    if busy {
        return Err(bad("Nhân viên đang làm việc, hãy tạm dừng dự án trước"));
    }
    store.remove_employee(&id);
    Ok(Json(json!({ "ok": true })))
}

async fn update_settings(State(store): State<S>, Json(mut s): Json<Settings>) -> ApiResult {
    s.max_concurrent = s.max_concurrent.clamp(1, 16);
    s.max_agent_steps = s.max_agent_steps.clamp(3, 200);
    s.max_bounces = s.max_bounces.min(10);
    s.mock_delay_ms = s.mock_delay_ms.min(10_000);
    store.resize_llm_slots(s.max_concurrent);
    store.with(|st| st.settings = s.clone());
    store.emit(ServerEvent::Settings { settings: s.clone() });
    Ok(Json(json!(s)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatBody {
    to: String,
    text: String,
    project_id: Option<String>,
}

async fn chairman_chat(State(store): State<S>, Json(b): Json<ChatBody>) -> ApiResult {
    if b.text.trim().is_empty() {
        return Err(bad("Tin nhắn trống"));
    }
    if store.employee(&b.to).is_none() {
        return Err(bad("Người nhận không tồn tại"));
    }
    let m = store.post_message(b.project_id.as_deref(), "chairman", &b.to, b.text.trim(), "chat");
    Ok(Json(json!(m)))
}
