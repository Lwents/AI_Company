//! Customer portal: a public page where customers contact the company and follow their case.
//! Only these routes (plus the product files) are exposed on the optional public listener.
use crate::ops;
use crate::store::{now_ms, S};
use crate::types::*;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

pub fn routes() -> Router<S> {
    Router::new()
        .route("/portal/{pid}/", get(page))
        .route("/portal/{pid}", get(page))
        .route("/portal/api/{pid}/cases", post(create))
        .route("/portal/api/{pid}/cases/{cid}", get(view))
        .route("/portal/api/{pid}/cases/{cid}/reply", post(reply))
        .route("/portal/api/{pid}/cases/{cid}/rate", post(rate))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn live(store: &S, pid: &str) -> Option<Project> {
    store.project(pid).filter(|p| matches!(p.status, ProjectStatus::Operating | ProjectStatus::Paused) && matches!(p.phase, Phase::Operating))
}

async fn page(State(store): State<S>, Path(pid): Path<String>) -> Response {
    let Some(p) = store.project(&pid) else { return (StatusCode::NOT_FOUND, "Không tìm thấy sản phẩm").into_response() };
    let open = live(&store, &pid).is_some();
    let landing = store.dir_of(&p).join("marketing").join("landing.html").exists();
    let html = PAGE
        .replace("{{NAME}}", &esc(&p.name))
        .replace("{{PID}}", &esc(&pid))
        .replace("{{LANDING}}", if landing { "" } else { "hidden" })
        .replace("{{OPEN}}", if open { "" } else { "hidden" })
        .replace("{{CLOSED}}", if open { "hidden" } else { "" });
    ([(header::CACHE_CONTROL, "no-store")], Html(html)).into_response()
}

#[derive(Deserialize)]
struct NewCase {
    name: String,
    #[serde(default)]
    contact: String,
    kind: CaseKind,
    #[serde(default)]
    subject: String,
    message: String,
}

async fn create(State(store): State<S>, Path(pid): Path<String>, Json(b): Json<NewCase>) -> Response {
    if live(&store, &pid).is_none() {
        return (StatusCode::BAD_REQUEST, "Sản phẩm chưa mở nhận yêu cầu").into_response();
    }
    if b.name.trim().is_empty() || b.message.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "Vui lòng nhập tên và nội dung").into_response();
    }
    // Simple flood guard: at most 30 open cases per product.
    let open = store.with(|s| s.cases.iter().filter(|c| c.project_id == pid && c.status != CaseStatus::Resolved).count());
    if open >= 30 {
        return (StatusCode::TOO_MANY_REQUESTS, "Hệ thống đang quá tải, vui lòng thử lại sau").into_response();
    }
    let subject = if b.subject.trim().is_empty() { b.message.chars().take(60).collect::<String>() } else { b.subject };
    let c = ops::new_case(&store, &pid, b.kind, &b.name, &b.contact, &subject, &b.message);
    Json(json!({ "id": c.id, "token": c.token })).into_response()
}

#[derive(Deserialize)]
struct Tok {
    t: String,
}

fn authorized(store: &S, pid: &str, cid: &str, t: &str) -> Option<SupportCase> {
    store.case(cid).filter(|c| c.project_id == pid && !t.is_empty() && c.token == t)
}

fn public_view(store: &S, c: &SupportCase) -> serde_json::Value {
    let msgs: Vec<_> = c
        .messages
        .iter()
        .map(|m| {
            let who = match m.from.as_str() {
                "customer" => c.customer.clone(),
                "system" => "Hệ thống".to_string(),
                "chairman" => "Ban giám đốc".to_string(),
                id => store.employee(id).map(|e| format!("{} · {}", e.name, crate::config::role_label(e.role))).unwrap_or_else(|| "Nhân viên".into()),
            };
            json!({ "mine": m.from == "customer", "who": who, "text": m.text, "ts": m.ts })
        })
        .collect();
    json!({ "id": c.id, "subject": c.subject, "status": c.status, "kind": c.kind, "rating": c.rating, "messages": msgs })
}

async fn view(State(store): State<S>, Path((pid, cid)): Path<(String, String)>, Query(q): Query<Tok>) -> Response {
    match authorized(&store, &pid, &cid, &q.t) {
        Some(c) => Json(public_view(&store, &c)).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Deserialize)]
struct ReplyBody {
    message: String,
}

async fn reply(State(store): State<S>, Path((pid, cid)): Path<(String, String)>, Query(q): Query<Tok>, Json(b): Json<ReplyBody>) -> Response {
    if authorized(&store, &pid, &cid, &q.t).is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let text: String = b.message.trim().chars().take(4000).collect();
    if text.is_empty() {
        return (StatusCode::BAD_REQUEST, "Nội dung trống").into_response();
    }
    store.update_case(&cid, |c| {
        c.messages.push(CaseMsg { from: "customer".into(), text, ts: now_ms() });
        // Escalated cases stay with the dev team, but support still answers the new message.
        c.status = CaseStatus::Open;
    });
    if let Some(c) = store.case(&cid) {
        if let Some(a) = &c.assignee {
            store.emit(ServerEvent::Handoff { from: "customer".into(), to: a.clone(), text: "☎ Khách nhắn tiếp".into() });
        }
        return Json(public_view(&store, &c)).into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

#[derive(Deserialize)]
struct RateBody {
    rating: u8,
}

async fn rate(State(store): State<S>, Path((pid, cid)): Path<(String, String)>, Query(q): Query<Tok>, Json(b): Json<RateBody>) -> Response {
    let Some(c) = authorized(&store, &pid, &cid, &q.t) else { return StatusCode::NOT_FOUND.into_response() };
    let r = b.rating.clamp(1, 5);
    if c.rating.is_some() {
        return (StatusCode::BAD_REQUEST, "Bạn đã đánh giá rồi").into_response();
    }
    store.update_case(&cid, |c| c.rating = Some(r));
    if let Some(a) = c.assignee {
        store.kpi(&a, |k| {
            k.csat_sum += r as u64;
            k.csat_count += 1;
        });
    }
    StatusCode::NO_CONTENT.into_response()
}

const PAGE: &str = r##"<!doctype html>
<html lang="vi">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>{{NAME}} — Hỗ trợ khách hàng</title>
<style>
  :root { --bg:#f4f6fb; --card:#fff; --text:#1c2333; --muted:#6b7488; --pri:#4f6bed; --line:#e3e7f0; --ok:#1f9d63; }
  * { box-sizing: border-box; }
  body { margin:0; font: 15px/1.5 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; background:var(--bg); color:var(--text); }
  header { background:#111a33; color:#fff; padding:22px 16px; }
  header .in, main { max-width:760px; margin:0 auto; }
  header h1 { margin:0; font-size:22px; }
  header p { margin:4px 0 0; color:#b8c2dc; }
  header a { color:#cfe0ff; margin-right:14px; font-size:14px; }
  main { padding:20px 16px 60px; display:grid; gap:16px; }
  .card { background:var(--card); border:1px solid var(--line); border-radius:14px; padding:18px; }
  h2 { margin:0 0 10px; font-size:18px; }
  label { display:block; font-size:13px; color:var(--muted); margin:10px 0 4px; }
  input, select, textarea { width:100%; padding:10px 12px; border:1px solid var(--line); border-radius:10px; font:inherit; background:#fff; color:inherit; }
  textarea { min-height:110px; resize:vertical; }
  button { background:var(--pri); color:#fff; border:0; border-radius:10px; padding:11px 18px; font:inherit; font-weight:600; cursor:pointer; margin-top:12px; }
  button:disabled { opacity:.5; cursor:default; }
  .row { display:grid; grid-template-columns:1fr 1fr; gap:12px; }
  .kinds { display:flex; gap:8px; flex-wrap:wrap; }
  .kinds label { margin:0; display:flex; gap:6px; align-items:center; border:1px solid var(--line); border-radius:999px; padding:6px 12px; color:var(--text); cursor:pointer; }
  .kinds input { width:auto; }
  .msgs { display:flex; flex-direction:column; gap:10px; margin:12px 0; }
  .msg { max-width:85%; padding:10px 12px; border-radius:12px; background:#eef1f8; white-space:pre-wrap; }
  .msg.mine { align-self:flex-end; background:#e3e9ff; }
  .msg .who { font-size:12px; color:var(--muted); margin-bottom:2px; }
  .status { display:inline-block; font-size:12px; padding:2px 10px; border-radius:999px; background:#eef1f8; }
  .status.resolved { background:#dcf5e9; color:var(--ok); }
  .stars button { background:#fff; color:#f5a524; border:1px solid var(--line); font-size:22px; padding:4px 10px; margin:6px 4px 0 0; }
  .muted { color:var(--muted); font-size:13px; }
  .hidden { display:none; }
  .link { word-break:break-all; background:#f6f8fc; border:1px dashed var(--line); padding:8px; border-radius:8px; font-size:13px; }
  @media (max-width:560px) { .row { grid-template-columns:1fr; } }
</style>
</head>
<body>
<header><div class="in">
  <h1>{{NAME}}</h1>
  <p>Trung tâm hỗ trợ khách hàng</p>
  <p><a href="/preview/{{PID}}/">Dùng sản phẩm</a><a class="{{LANDING}}" href="/preview/{{PID}}/marketing/landing.html">Giới thiệu</a></p>
</div></header>
<main>
  <section class="card {{CLOSED}}"><h2>Tạm ngừng nhận yêu cầu</h2><p class="muted">Sản phẩm chưa ra mắt hoặc đang bảo trì. Vui lòng quay lại sau.</p></section>

  <section class="card {{OPEN}}" id="newCard">
    <h2>Gửi yêu cầu</h2>
    <div class="kinds">
      <label><input type="radio" name="kind" value="support" checked /> Cần hỗ trợ</label>
      <label><input type="radio" name="kind" value="bug" /> Báo lỗi</label>
      <label><input type="radio" name="kind" value="sales" /> Tư vấn mua hàng</label>
    </div>
    <div class="row">
      <div><label>Tên của bạn *</label><input id="name" maxlength="60" /></div>
      <div><label>Email hoặc số điện thoại</label><input id="contact" maxlength="100" /></div>
    </div>
    <label>Tiêu đề</label><input id="subject" maxlength="140" />
    <label>Nội dung *</label><textarea id="message" maxlength="4000"></textarea>
    <button id="send">Gửi</button>
    <p class="muted" id="err"></p>
  </section>

  <section class="card hidden" id="caseCard">
    <h2 id="cSubject"></h2>
    <span class="status" id="cStatus"></span>
    <p class="muted">Lưu lại đường link này để xem phản hồi bất cứ lúc nào:</p>
    <div class="link" id="cLink"></div>
    <div class="msgs" id="cMsgs"></div>
    <div id="replyBox">
      <textarea id="reply" placeholder="Nhắn thêm…"></textarea>
      <button id="sendReply">Gửi</button>
    </div>
    <div id="rateBox" class="hidden">
      <p>Bạn hài lòng với hỗ trợ của chúng tôi chứ?</p>
      <div class="stars" id="stars"></div>
    </div>
    <p class="muted"><a href="/portal/{{PID}}/">Gửi yêu cầu khác</a></p>
  </section>
</main>
<script>
const PID = "{{PID}}";
const $ = (id) => document.getElementById(id);
const STATUS = { open: "Đang chờ nhân viên", in_progress: "Nhân viên đang trả lời…", waiting_customer: "Đã phản hồi", escalated: "Đội kỹ thuật đang xử lý", resolved: "Đã giải quyết" };
let caseId = null, token = null, timer = null;

async function api(method, url, body) {
  const r = await fetch(url, { method, headers: body ? { "content-type": "application/json" } : {}, body: body ? JSON.stringify(body) : undefined });
  const t = await r.text();
  if (!r.ok) throw new Error(t || r.statusText);
  return t ? JSON.parse(t) : null;
}

function render(c) {
  $("newCard").classList.add("hidden");
  $("caseCard").classList.remove("hidden");
  $("cSubject").textContent = c.subject;
  $("cStatus").textContent = STATUS[c.status] || c.status;
  $("cStatus").className = "status " + c.status;
  $("cLink").textContent = location.origin + location.pathname + "#c=" + caseId + "&t=" + token;
  const box = $("cMsgs");
  box.innerHTML = "";
  for (const m of c.messages) {
    const d = document.createElement("div");
    d.className = "msg" + (m.mine ? " mine" : "");
    const w = document.createElement("div");
    w.className = "who";
    w.textContent = m.who + " · " + new Date(m.ts).toLocaleString("vi-VN");
    d.append(w, document.createTextNode(m.text));
    box.append(d);
  }
  const done = c.status === "resolved";
  $("rateBox").classList.toggle("hidden", !done || c.rating);
  if (done && !c.rating) {
    const s = $("stars");
    s.innerHTML = "";
    for (let i = 1; i <= 5; i++) {
      const b = document.createElement("button");
      b.textContent = "★".repeat(i);
      b.onclick = async () => { await api("POST", `/portal/api/${PID}/cases/${caseId}/rate?t=${token}`, { rating: i }); load(); };
      s.append(b);
    }
  }
}

async function load() {
  try { render(await api("GET", `/portal/api/${PID}/cases/${caseId}?t=${token}`)); } catch (e) { $("cSubject").textContent = "Không tìm thấy yêu cầu"; }
}

function open(id, t) {
  caseId = id; token = t;
  location.hash = "c=" + id + "&t=" + t;
  load();
  clearInterval(timer);
  timer = setInterval(load, 2500);
}

$("send").onclick = async () => {
  $("err").textContent = "";
  const kind = document.querySelector("input[name=kind]:checked").value;
  try {
    $("send").disabled = true;
    const r = await api("POST", `/portal/api/${PID}/cases`, { name: $("name").value, contact: $("contact").value, subject: $("subject").value, message: $("message").value, kind });
    open(r.id, r.token);
  } catch (e) { $("err").textContent = e.message; }
  finally { $("send").disabled = false; }
};

$("sendReply").onclick = async () => {
  const text = $("reply").value.trim();
  if (!text) return;
  render(await api("POST", `/portal/api/${PID}/cases/${caseId}/reply?t=${token}`, { message: text }));
  $("reply").value = "";
};

const h = new URLSearchParams(location.hash.slice(1));
if (h.get("c") && h.get("t")) open(h.get("c"), h.get("t"));
</script>
</body>
</html>
"##;
