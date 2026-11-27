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
        .route("/api/uploads", post(upload_source).layer(axum::extract::DefaultBodyLimit::max(300 * 1024 * 1024)))
        .route("/api/projects/{id}", axum::routing::delete(delete_project))
        .route("/api/projects/{id}/files", get(project_files))
        .route("/api/projects/{id}/file", get(project_file))
        .route("/api/projects/{id}/file/raw", get(project_file_raw))
        .route("/api/projects/{id}/download", get(download_source))
        .route("/api/projects/{id}/versions/{n}", get(download_version))
        .route("/api/projects/{id}/{action}", post(project_action))
        .route("/api/approvals/{id}", post(resolve_approval))
        .route("/api/employees", post(create_employee))
        .route("/api/employees/{id}", put(update_employee).delete(delete_employee))
        .route("/api/skills", get(list_skills))
        .route("/login", get(crate::auth::login_page))
        .route("/api/auth/login", post(crate::auth::login))
        .route("/api/auth/logout", post(crate::auth::logout))
        .route("/api/auth/status", get(crate::auth::status))
        .route("/api/resources", get(resource_accounts))
        .route("/api/resources/{src}/{id}", get(resource_quota))
        .route("/api/settings", put(update_settings))
        .route("/api/tools-skill", get(get_tools_skill).put(put_tools_skill).delete(delete_tools_skill))
        .route("/api/mcp", get(mcp_servers))
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
    #[serde(default)]
    tech: String,
    /// Id returned by POST /api/uploads (a .zip of the customer's code).
    #[serde(default)]
    upload_id: String,
    #[serde(default)]
    upload_name: String,
    /// Or: a Git repository to clone.
    #[serde(default)]
    git_url: String,
    /// Or: a folder on this computer (copied, the original is left alone).
    #[serde(default)]
    local_path: String,
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
        design_reviews: 0,
        qa_rounds: 0,
        security_reviews: 0,
        auto_retries: 0,
        meetings: 0,
        tech: body.tech.trim().chars().take(300).collect(),
        versions: vec![],
        existing: false,
        source: String::new(),
        dir: String::new(),
        created_at: now_ms(),
        updated_at: now_ms(),
    };
    let mut p = p;
    // The customer's existing code goes into the project folder before anyone starts.
    let upload_id: String = body.upload_id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let local = body.local_path.trim().trim_matches('"').to_string();
    if !upload_id.is_empty() || !body.git_url.trim().is_empty() || !local.is_empty() {
        let dir = store.workspace_dir.join(&p.slug);
        let res = if !upload_id.is_empty() {
            let file = store.data_dir.join("uploads").join(format!("{upload_id}.zip"));
            let bytes = tokio::fs::read(&file).await.map_err(|_| bad("Không tìm thấy tệp đã tải lên, hãy tải lại"))?;
            let d = dir.clone();
            let r = tokio::task::spawn_blocking(move || crate::import::extract_zip(&bytes, &d)).await.map_err(|e| bad(e.to_string()))?;
            let _ = tokio::fs::remove_file(&file).await;
            p.source = if body.upload_name.trim().is_empty() { "Tệp .zip".into() } else { body.upload_name.trim().chars().take(120).collect() };
            r.map(|_| ())
        } else if !local.is_empty() {
            // Work directly in the customer's folder (a v1 zip of the original is kept in the history).
            let root = store.data_dir.parent().map(|r| r.to_path_buf()).unwrap_or_default();
            crate::import::check_local(std::path::Path::new(&local), &[root]).map(|abs| {
                p.source = abs.display().to_string();
                p.dir = abs.display().to_string();
            })
        } else {
            p.source = body.git_url.trim().to_string();
            crate::import::git_clone(&body.git_url, &dir).await
        };
        if let Err(e) = res {
            // Only ever clean up our own workspace folder, never the customer's.
            if p.dir.is_empty() {
                let _ = tokio::fs::remove_dir_all(&dir).await;
            }
            return Err(bad(e));
        }
        p.existing = true;
    }
    store.insert_project(p.clone());
    // Keep the original as v1, except for the customer's own folder under Git (Git already has the history).
    let under_git = !p.dir.is_empty() && std::path::Path::new(&p.dir).join(".git").exists();
    if p.existing && !under_git {
        let label = if p.dir.is_empty() { "Mã nguồn gốc của khách" } else { "Bản gốc trước khi sửa (thư mục của khách)" };
        crate::history::snapshot(&store, &id, label).await;
    }
    workflow::start(&store, &id);
    Ok(Json(json!(store.project(&id).unwrap_or(p))))
}

/// A customer's source code (.zip, raw body). Returns an id to pass to POST /api/projects.
async fn upload_source(State(store): State<S>, body: axum::body::Bytes) -> ApiResult {
    if body.len() < 22 || &body[..2] != b"PK" {
        return Err(bad("Chỉ nhận tệp .zip"));
    }
    let id = new_id();
    let dir = store.data_dir.join("uploads");
    tokio::fs::create_dir_all(&dir).await.map_err(|e| bad(e.to_string()))?;
    tokio::fs::write(dir.join(format!("{id}.zip")), &body).await.map_err(|e| bad(e.to_string()))?;
    Ok(Json(json!({ "id": id, "size": body.len() })))
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
                p.design_reviews = 0;
                p.qa_rounds = 0;
                p.security_reviews = 0;
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
    Ok(store.dir_of(&p))
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

fn is_image_ext(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp" | "svg")
}

fn is_binary_ext(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    matches!(
        ext.as_str(),
        "apk" | "zip" | "gz" | "tar" | "xz" | "7z" | "rar" | "exe" | "dll" | "so" | "dylib" | "pdf" | "woff" | "woff2" | "ttf" | "otf" | "mp3" | "mp4" | "wav" | "ogg" | "webm" | "pyc" | "pyd" | "wasm" | "db" | "sqlite" | "sqlite3" | "bin"
    )
}

async fn project_file(State(store): State<S>, Path(id): Path<String>, Query(q): Query<FileQuery>) -> ApiResult {
    let dir = project_dir(&store, &id)?;
    let p = safe_path(&dir, &q.path).map_err(bad)?;
    let meta = tokio::fs::metadata(&p).await.map_err(|e| bad(e.to_string()))?;
    let size = meta.len();
    if is_image_ext(&q.path) {
        return Ok(Json(json!({ "path": q.path, "kind": "image", "size": size, "mime": mime(&q.path), "content": "" })));
    }
    if is_binary_ext(&q.path) {
        return Ok(Json(json!({ "path": q.path, "kind": "binary", "size": size, "mime": mime(&q.path), "content": "" })));
    }
    let bytes = tokio::fs::read(&p).await.map_err(|e| bad(e.to_string()))?;
    let probe = &bytes[..bytes.len().min(4096)];
    if probe.contains(&0) {
        return Ok(Json(json!({ "path": q.path, "kind": "binary", "size": size, "mime": mime(&q.path), "content": "" })));
    }
    const MAX_PREVIEW_CHARS: usize = 50_000;
    let decoded = String::from_utf8_lossy(&bytes);
    let truncated = decoded.chars().count() > MAX_PREVIEW_CHARS;
    let text: String = decoded.chars().take(MAX_PREVIEW_CHARS).collect();
    Ok(Json(json!({ "path": q.path, "kind": "text", "size": size, "truncated": truncated, "content": text })))
}

async fn project_file_raw(State(store): State<S>, Path(id): Path<String>, Query(q): Query<FileQuery>) -> Response {
    let Ok(dir) = project_dir(&store, &id) else { return StatusCode::NOT_FOUND.into_response() };
    let Ok(p) = safe_path(&dir, &q.path) else { return StatusCode::BAD_REQUEST.into_response() };
    match tokio::fs::read(&p).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, mime(&q.path)), (header::CACHE_CONTROL, "no-store")], bytes).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Không tìm thấy tệp").into_response(),
    }
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

fn zip_response(name: &str, bytes: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\"")),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        bytes,
    )
        .into_response()
}

/// The project's current source code as a zip.
async fn download_source(State(store): State<S>, Path(id): Path<String>) -> Response {
    let Some(p) = store.project(&id) else { return (StatusCode::NOT_FOUND, "Dự án không tồn tại").into_response() };
    let dir = store.dir_of(&p);
    let slug = p.slug.clone();
    match tokio::task::spawn_blocking(move || crate::history::zip_dir(&dir, &slug)).await {
        Ok(Ok((bytes, _))) => zip_response(&format!("{}.zip", p.slug), bytes),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// A frozen version from the project's history.
async fn download_version(State(store): State<S>, Path((id, n)): Path<(String, u32)>) -> Response {
    let Some(p) = store.project(&id) else { return (StatusCode::NOT_FOUND, "Dự án không tồn tại").into_response() };
    match tokio::fs::read(crate::history::version_file(&store, &p.slug, n)).await {
        Ok(bytes) => zip_response(&format!("{}-v{n}.zip", p.slug), bytes),
        Err(_) => (StatusCode::NOT_FOUND, "Không tìm thấy phiên bản").into_response(),
    }
}

async fn serve_preview(store: S, id: String, path: String) -> Response {
    let Ok(mut dir) = project_dir(&store, &id) else { return StatusCode::NOT_FOUND.into_response() };
    // Projects with a build step (Vite, …) are previewed from their build output (root or frontend/).
    for out in ["dist", "build", "frontend/dist", "frontend/build", "frontend"] {
        let built = dir.join(out);
        if built.join("index.html").is_file() {
            let rel = if path.is_empty() || path.ends_with('/') { format!("{path}index.html") } else { path.clone() };
            // Only fall back to the source tree for files the build doesn't have (design/mockup.html…).
            if built.join(&rel).is_file() || !dir.join(&rel).is_file() {
                dir = built;
            }
            break;
        }
    }
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

async fn preview(State(store): State<S>, Path((id, path)): Path<(String, String)>, public: Option<axum::Extension<crate::auth::PublicPreview>>) -> Response {
    if public.is_some() && hidden_from_public_preview(&path) {
        return (StatusCode::NOT_FOUND, "Không tìm thấy tệp").into_response();
    }
    serve_preview(store, id, path).await
}

/// Files a preview opened without login never serves: documents, tests, VCS data, secrets.
fn hidden_from_public_preview(path: &str) -> bool {
    let p = path.to_lowercase().replace('\\', "/");
    const HIDDEN: [&str; 9] = ["docs/", "finance/", "hr/", "sales/", "support/", "legal/notes", "tests/", ".git/", "node_modules/"];
    HIDDEN.iter().any(|h| p.starts_with(h))
        || p.split('/').any(|seg| seg.starts_with(".env") || seg == ".git")
        || (p.ends_with(".md") && !p.starts_with("legal/"))
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

async fn list_skills(State(store): State<S>) -> ApiResult {
    let root = store.data_dir.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let files: Vec<Value> = crate::skills::list_files(&root)
        .into_iter()
        .map(|(name, path, content)| json!({ "name": name, "path": path, "content": content }))
        .collect();
    Ok(Json(json!({ "files": files })))
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
    e.skill_file = e.skill_file.trim().trim_matches('"').to_string();
    if !e.skill_file.is_empty() {
        let root = store.data_dir.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        crate::skills::read_skill_file(&root, &e.skill_file).map_err(bad)?;
    }
    e.skill_text = e.skill_text.trim().chars().take(crate::skills::MAX_SKILL_CHARS).collect();
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

fn company_root(store: &S) -> std::path::PathBuf {
    store.data_dir.parent().map(|p| p.to_path_buf()).unwrap_or_default()
}

/// MCP servers from mcp.json with their current on/off state, for the Settings tab.
async fn mcp_servers(State(store): State<S>) -> Json<Value> {
    let overrides = store.with(|s| s.settings.mcp.clone());
    Json(json!({ "servers": crate::mcp::overview(&company_root(&store), &overrides) }))
}

async fn update_settings(State(store): State<S>, Json(mut s): Json<Settings>) -> ApiResult {
    s.max_concurrent = s.max_concurrent.clamp(1, 16);
    s.meeting_rounds = s.meeting_rounds.min(5);
    s.auto_retries = s.auto_retries.min(10);
    s.fallback_model = s.fallback_model.trim().to_string();
    s.light_model = s.light_model.trim().to_string();
    s.max_agent_steps = s.max_agent_steps.clamp(3, 200);
    s.max_bounces = s.max_bounces.min(10);
    s.mock_delay_ms = s.mock_delay_ms.min(10_000);
    s.tools_skill_file = s.tools_skill_file.trim().trim_matches('"').to_string();
    if s.tools_mode {
        crate::skills::read_tools_skill_file(&company_root(&store), &s.tools_skill_file).map_err(bad)?;
    }
    // Keep only servers that exist in mcp.json.
    let known = crate::mcp::load_config(&company_root(&store));
    s.mcp.retain(|name, _| known.contains_key(name));
    store.resize_llm_slots(s.max_concurrent);
    store.with(|st| st.settings = s.clone());
    store.emit(ServerEvent::Settings { settings: s.clone() });
    Ok(Json(json!(s)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolsSkillBody {
    #[serde(default)]
    name: String,
    content: String,
}

async fn get_tools_skill(State(store): State<S>) -> ApiResult {
    let root = company_root(&store);
    let rel = store.with(|s| s.settings.tools_skill_file.clone());
    if let Some(found) = crate::skills::resolve_tools_skill_path(&root, &rel) {
        let content = crate::skills::read_tools_skill_file(&root, &rel).unwrap_or_default();
        let name = found.file_name().and_then(|s| s.to_str()).unwrap_or(&rel).to_string();
        let chars = content.chars().count();
        Ok(Json(json!({ "exists": true, "name": name, "path": rel, "chars": chars, "content": content })))
    } else {
        Ok(Json(json!({ "exists": false, "name": "", "path": "", "chars": 0, "content": "" })))
    }
}

async fn put_tools_skill(State(store): State<S>, Json(b): Json<ToolsSkillBody>) -> ApiResult {
    let root = company_root(&store);
    let rel = crate::skills::save_tools_skill(&root, &b.name, &b.content).map_err(bad)?;
    let content = crate::skills::read_tools_skill_file(&root, &rel).map_err(bad)?;
    let name = std::path::Path::new(&rel).file_name().and_then(|s| s.to_str()).unwrap_or(&rel).to_string();
    let chars = content.chars().count();
    let updated = store.with(|st| {
        st.settings.tools_skill_file = rel.clone();
        st.settings.clone()
    });
    store.emit(ServerEvent::Settings { settings: updated });
    Ok(Json(json!({ "exists": true, "name": name, "path": rel, "chars": chars, "content": content })))
}

async fn delete_tools_skill(State(store): State<S>) -> ApiResult {
    let root = company_root(&store);
    let current = store.with(|s| s.settings.tools_skill_file.clone());
    crate::skills::delete_tools_skill(&root, &current).map_err(bad)?;
    let updated = store.with(|st| {
        st.settings.tools_mode = false;
        st.settings.tools_skill_file.clear();
        st.settings.clone()
    });
    store.emit(ServerEvent::Settings { settings: updated });
    Ok(Json(json!({ "exists": false, "name": "", "path": "", "chars": 0, "content": "" })))
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

async fn resource_accounts() -> ApiResult {
    Ok(Json(crate::resources::sources().await))
}

async fn resource_quota(Path((src, id)): Path<(usize, String)>) -> ApiResult {
    if !crate::resources::valid_id(&id) {
        return Err(bad("Mã tài khoản không hợp lệ"));
    }
    crate::resources::quota(src, &id).await.map(Json).map_err(|e| (StatusCode::BAD_GATEWAY, e))
}

#[cfg(test)]
mod preview_tests {
    #[test]
    fn public_preview_hides_internal_files() {
        use super::hidden_from_public_preview as h;
        assert!(h("docs/PLAN.md") && h(".git/config") && h("a/.env") && h(".env.local") && h("README.md") && h("node_modules/x.js"));
        assert!(!h("index.html") && !h("assets/app.js") && !h("design/mockup.html") && !h("legal/terms.md"));
    }
}
