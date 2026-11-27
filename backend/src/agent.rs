//! One AI employee doing one job: a tool-use loop until a terminal tool is called.
use crate::config;
use crate::llm::{self, LlmMessage, LlmRequest, Meta, Stop, ToolResult};
use crate::store::{now_ms, S};
use crate::tools::{self, Ctx, Outcome};
use crate::types::*;
use serde_json::Value;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct Job {
    pub employee_id: String,
    pub project_id: String,
    pub phase: &'static str,
    pub ticket_id: Option<String>,
    pub case_id: Option<String>,
    pub task: String,
}

pub struct JobResult {
    pub outcome: Outcome,
    pub created_tickets: u32,
}

fn system_prompt(e: &Employee, project: &Project, dir: &PathBuf) -> String {
    let mut s = format!(
        "{duty}\n\nBạn tên {name}, làm việc tại một công ty phần mềm vận hành bởi các AI. Chủ tịch (người dùng) giao dự án; CEO điều phối; mỗi người làm đúng vai trò.\n\n\
Dự án: {pname}\nThư mục dự án: {dir} (mọi đường dẫn trong tool là tương đối so với thư mục này).\n\n\
Quy tắc:\n\
- Làm việc bằng tool. Đọc tài liệu trong docs/ trước khi làm nếu cần ngữ cảnh.\n\
- Với tệp dài (> 200 dòng): dùng read_file kèm start_line/end_line để đọc đúng đoạn và ưu tiên edit_file để sửa từng chỗ thay vì viết lại cả tệp bằng write_file.\n\
- Ưu tiên giải pháp đơn giản, chạy được ngay, ít phụ thuộc.\n\
- Không đọc/ghi ra ngoài thư mục dự án. Không đưa API key vào code.\n\
- Viết bằng tiếng Việt (code và tên tệp bằng tiếng Anh).\n\
- Khi xong việc, PHẢI gọi tool kết thúc (finish / pass_ticket / fail_ticket / approve_code / request_changes) kèm tóm tắt ngắn.",
        duty = config::role_duty(e.role),
        name = e.name,
        pname = project.name,
        dir = dir.display(),
    );
    // The project's non-negotiable principles apply to everyone, every step.
    if let Ok(c) = std::fs::read_to_string(dir.join("docs").join("CONSTITUTION.md")) {
        let c: String = c.trim().chars().take(4000).collect();
        if !c.is_empty() {
            s.push_str("\n\n# NGUYÊN TẮC DỰ ÁN (docs/CONSTITUTION.md — bắt buộc tuân thủ, không được vi phạm)\n");
            s.push_str(&c);
        }
    }
    if !e.extra_prompt.trim().is_empty() {
        s.push_str("\n\nChỉ dẫn thêm từ Chủ tịch:\n");
        s.push_str(e.extra_prompt.trim());
    }
    s
}

/// New messages addressed to this employee since their last job.
fn take_inbox(store: &S, emp_id: &str, project_id: &str) -> Option<String> {
    let since = store.inbox_read.lock().unwrap().get(emp_id).copied().unwrap_or(0);
    let msgs: Vec<String> = store.with(|s| {
        s.messages
            .iter()
            .filter(|m| m.ts > since && m.to == emp_id && m.kind == "chat" && m.project_id.as_deref().map_or(true, |p| p == project_id))
            .map(|m| (m.from.clone(), m.text.clone()))
            .collect::<Vec<_>>()
    })
    .into_iter()
    .map(|(from, text)| format!("- {}: {}", store.display_name(&from), text))
    .collect();
    store.inbox_read.lock().unwrap().insert(emp_id.to_string(), now_ms());
    (!msgs.is_empty()).then(|| format!("\n\nTin nhắn mới gửi bạn:\n{}", msgs.join("\n")))
}

/// Session pruning (OpenClaw): only the latest tool turns keep their full output. Older tool
/// results (whole files, long command logs…) shrink to head + tail so every later model call is
/// cheaper and more focused. The beginning and end survive, so exit codes and errors stay visible.
const KEEP_FULL_TOOL_TURNS: usize = 3;
const PRUNED_TOOL_CHARS: usize = 2000;

fn image_mime(path: &str) -> Option<&'static str> {
    match path.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

/// An image file as (mime, base64) for the model; skipped when missing or too big (> 4 MB).
fn image_attachment(path: &std::path::Path) -> Option<(String, String)> {
    let mime = image_mime(&path.to_string_lossy())?;
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > 4 * 1024 * 1024 {
        return None;
    }
    Some((mime.to_string(), base64(&std::fs::read(path).ok()?)))
}

/// Where a screenshot with a filename was saved: the `path: '…'` Playwright prints, else the `filename` argument.
fn saved_screenshot(dir: &std::path::Path, text: &str, input: &Value) -> Option<(String, String)> {
    let printed = text.split("path: '").nth(1).and_then(|s| s.split('\'').next()).map(|s| s.replace("\\\\", "/").replace('\\', "/"));
    let rel = printed.or_else(|| input.get("filename").and_then(Value::as_str).map(str::to_string))?;
    image_attachment(&tools::safe_path(dir, &rel).ok()?)
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for ch in data.chunks(3) {
        let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= ch.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn prune_tool_results(messages: &mut [LlmMessage], keep: usize) {
    let mut seen = 0;
    for m in messages.iter_mut().rev() {
        let LlmMessage::User { tool_results, .. } = m else { continue };
        if tool_results.is_empty() {
            continue;
        }
        seen += 1;
        if seen <= keep {
            continue;
        }
        for r in tool_results.iter_mut() {
            r.images.clear();
            let n = r.content.chars().count();
            if n > PRUNED_TOOL_CHARS {
                let head: String = r.content.chars().take(PRUNED_TOOL_CHARS * 6 / 10).collect();
                let tail: String = r.content.chars().skip(n - PRUNED_TOOL_CHARS * 3 / 10).collect();
                r.content = format!("{head}
…[đã rút gọn {} ký tự của kết quả cũ; đọc lại tệp nếu cần]…
{tail}", n - head.chars().count() - tail.chars().count());
            }
        }
    }
}

async fn budget_gate(store: &S, project_id: &str, emp_id: &str, cancel: &CancellationToken) -> Result<(), String> {
    loop {
        let Some(p) = store.project(project_id) else { return Err("Dự án không tồn tại".into()) };
        if p.budget_usd <= 0.0 || p.spent_usd < p.budget_usd {
            return Ok(());
        }
        let step = store.with(|s| s.settings.default_budget_usd).max(0.5);
        let a = store
            .request_approval(
                Some(project_id),
                emp_id,
                ApprovalKind::Budget,
                "Hết ngân sách dự án",
                format!(
                    "Dự án \"{}\" đã tiêu ${:.4} / ${:.2}. Duyệt để cấp thêm ${:.2}.",
                    p.name, p.spent_usd, p.budget_usd, step
                ),
                cancel,
            )
            .await?;
        if a.status != ApprovalStatus::Approved {
            return Err("Chủ tịch không cấp thêm ngân sách".into());
        }
        store.update_project(project_id, |p| p.budget_usd += step);
    }
}

fn short(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}…")
    } else {
        t
    }
}

fn describe_call(name: &str, input: &Value) -> String {
    let g = |k: &str| input.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    if let Some((server, tool)) = name.split_once("__") {
        let what = match tool {
            "browser_navigate" => format!("Mở {}", g("url")),
            "browser_take_screenshot" => "Chụp màn hình".into(),
            "browser_snapshot" => "Đọc trang".into(),
            "browser_click" => format!("Bấm {}", g("element")),
            "browser_type" => format!("Gõ vào {}", g("element")),
            "browser_resize" => "Đổi cỡ màn hình".into(),
            "desktop_list_windows" => "Xem cửa sổ trên máy".into(),
            "desktop_launch" => format!("Mở ứng dụng {}", g("app")),
            "desktop_close_app" => format!("Đóng ứng dụng {}", g("app")),
            "desktop_focus_window" => format!("Chuyển cửa sổ {}", g("app")),
            "desktop_snapshot" => "Đọc UI cửa sổ".into(),
            "desktop_screenshot" => "Chụp ảnh cửa sổ".into(),
            "desktop_click" => format!("Bấm {}", g("ref")),
            "desktop_type" => format!("Gõ vào {}", g("ref")),
            _ => tool.replace('_', " "),
        };
        return format!("[{server}] {what}");
    }
    match name {
        "write_file" => format!("Ghi {}", g("path")),
        "edit_file" => format!("Sửa {}", g("path")),
        "read_file" => match (input.get("start_line").and_then(|v| v.as_u64()), input.get("end_line").and_then(|v| v.as_u64())) {
            (Some(s), Some(e)) => format!("Đọc {} (L{s}-L{e})", g("path")),
            (Some(s), None) => format!("Đọc {} (từ L{s})", g("path")),
            _ => format!("Đọc {}", g("path")),
        },
        "list_files" => "Xem danh sách tệp".into(),
        "read_skill" => match g("file").trim() {
            "" => format!("Đọc skill {}", g("name")),
            f => format!("Đọc skill {} › {f}", g("name")),
        },
        "run_command" => format!("$ {}", g("command")),
        "create_ticket" => format!("Tạo ticket: {}", g("title")),
        "message_colleague" => format!("Nhắn {}", g("to_role")),
        "ask_chairman" => "Hỏi Chủ tịch".into(),
        "finish" => "Hoàn thành, bàn giao".into(),
        "set_priority" => format!("Ưu tiên {} → {}", g("key"), input.get("priority").map(|v| v.to_string()).unwrap_or_default()),
        "report_issue" => format!("Báo lỗi phát hành: {}", g("title")),
        "reply_customer" => "Trả lời khách hàng".into(),
        "resolve_case" => "Đóng hồ sơ khách hàng".into(),
        "escalate_bug" => format!("Chuyển lỗi cho kỹ thuật: {}", g("title")),
        "propose_deal" => format!("Đề xuất chốt đơn: {}", g("plan")),
        "company_finance" => "Xem số liệu tài chính".into(),
        "company_kpi" => "Xem KPI nhân viên".into(),
        "propose_hire" => format!("Đề xuất tuyển {}", g("role")),
        "pass_ticket" => "Kết luận: ticket đạt".into(),
        "fail_ticket" => "Kết luận: ticket còn lỗi".into(),
        "approve_code" => "Duyệt code".into(),
        "request_changes" => "Yêu cầu sửa code".into(),
        other => other.to_string(),
    }
}

/// Conversation with a customer, as plain text for the prompt.
pub fn case_transcript(store: &S, c: &SupportCase) -> String {
    let mut s = format!(
        "Hồ sơ {} · loại {:?} · trạng thái {:?}\nKhách hàng: {}{}\nTiêu đề: {}\n\nHội thoại:\n",
        c.id,
        c.kind,
        c.status,
        c.customer,
        if c.contact.is_empty() { String::new() } else { format!(" ({})", c.contact) },
        c.subject
    );
    for m in &c.messages {
        let who = match m.from.as_str() {
            "customer" => format!("Khách {}", c.customer),
            "system" => "Hệ thống".into(),
            other => store.display_name(other),
        };
        s.push_str(&format!("- {who}: {}\n", m.text));
    }
    s
}

/// Detects policy refusals returned as normal text (`finish_reason = stop`) by OpenAI-compatible proxies.
pub fn looks_like_refusal(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    if t.is_empty() {
        return false;
    }
    let head: String = t.chars().take(280).collect();
    const PHRASES: [&str; 15] = [
        "sorry, i cannot",
        "sorry, i can't",
        "i'm sorry, i cannot",
        "i'm sorry, i can't",
        "i apologize, but i cannot",
        "i apologize, but i can't",
        "i cannot fulfill",
        "i can't fulfill",
        "i cannot assist with",
        "i cannot help with",
        "i am unable to fulfill",
        "i'm unable to fulfill",
        "tôi xin lỗi, tôi không thể",
        "xin lỗi, tôi không thể",
        "tôi không thể đáp ứng yêu cầu",
    ];
    PHRASES.iter().any(|p| head.contains(p))
}

pub fn is_refusal_err(e: &str) -> bool {
    e.contains("từ chối yêu cầu")
}

/// Errors a different person, model or wording can get past (model / provider trouble, refusals, running out of steps).
fn recoverable(e: &str) -> bool {
    const PROVIDERS: [&str; 6] = ["Custom", "Openai", "Openrouter", "Gemini", "Anthropic", "Ollama"];
    e != "aborted"
        && (PROVIDERS.iter().any(|p| e.starts_with(p))
            || is_refusal_err(e)
            || e.contains("bước mà chưa xong")
            || e.contains("không trả lời")
            || e.contains("không gọi công cụ kết thúc")
            || e.contains("không gọi finish"))
}

/// Who steps in when someone is stuck: Tech Lead for engineers, otherwise the CEO (then the PM).
fn supervisor(store: &S, of: &Employee) -> Option<Employee> {
    let order: &[Role] = match of.role {
        Role::Dev | Role::Reviewer | Role::Tester | Role::Devops | Role::DataEngineer | Role::Security => &[Role::Architect, Role::Ceo, Role::Pm],
        Role::Ceo => &[Role::Pm, Role::Architect],
        _ => &[Role::Ceo, Role::Pm],
    };
    order.iter().filter_map(|r| store.staff(*r).into_iter().find(|e| e.id != of.id)).next()
}

/// The supervisor rewrites the assignment: clearer, smaller steps, neutral professional wording, same goal.
async fn rephrase(store: &S, sup: &Employee, stuck: &Employee, job: &Job, err: &str, cancel: &CancellationToken) -> Option<String> {
    let meta = Meta {
        phase: "rephrase",
        project_id: job.project_id.clone(),
        extra: String::new(),
        project_name: String::new(),
        request: String::new(),
        ticket: None,
    };
    let system = format!("{}\nBạn đang hỗ trợ một nhân viên gặp sự cố khi làm việc.", config::role_duty(sup.role));
    let ask = format!(
        "{} ({}) không hoàn thành được việc dưới đây do sự cố: {}\n\nViết lại lời giao việc cho rõ ràng và dễ làm hơn: nêu bối cảnh nghiệp vụ của dự án, chia thành các bước nhỏ theo thứ tự, câu chữ trung tính và chuyên nghiệp, nói rõ cần đọc tệp nào và kết thúc bằng công cụ nào. GIỮ NGUYÊN mục tiêu, phạm vi và mọi tiêu chí nghiệm thu — không bỏ hay đổi yêu cầu nào. Chỉ trả về nội dung giao việc mới.\n\n--- Giao việc gốc ---\n{}",
        stuck.name,
        config::role_label(stuck.role),
        short(err, 300),
        job.task
    );
    let messages = vec![LlmMessage::User { text: Some(ask), tool_results: vec![] }];
    let res = llm::chat(store, LlmRequest { employee: sup, system: &system, messages: &messages, tools: &[], max_tokens: 8000, meta: &meta }, cancel).await.ok()?;
    store.add_usage(&sup.id, Some(&job.project_id), res.input_tokens, res.output_tokens);
    let t = res.text.trim().to_string();
    (t.chars().count() > 40).then_some(t)
}

/// Runs a job; if it fails for a recoverable reason the supervisor steps in (up to twice):
/// 1st: hand it to a colleague with the same role (skipping same-model colleagues on refusal), or to the backup model;
/// 2nd: reword the assignment and try again (on the backup model if there is one).
/// Jobs that don't need the strongest model.
const LIGHT_PHASES: [&str; 9] = [
    "meeting_opinion",
    "meeting_ballot",
    "meeting_vote",
    "minutes",
    "standup",
    "planning",
    "clarify",
    "consult",
    "retro",
];

pub async fn run(store: &S, job: Job, cancel: &CancellationToken) -> Result<JobResult, String> {
    let mut job = job;
    let light = store.with(|s| s.settings.light_model.trim().to_string());
    let mut model: Option<String> = (!light.is_empty() && LIGHT_PHASES.contains(&job.phase)).then_some(light);
    let original = job.employee_id.clone();
    for attempt in 0..3 {
        let r = run_tracked(store, job.clone(), model.as_deref(), cancel).await;
        let err = match &r {
            Err(e) if attempt < 2 && recoverable(e) && !cancel.is_cancelled() => e.clone(),
            _ => return r,
        };
        let Some(stuck) = store.employee(&job.employee_id) else { return r };
        let current_model = model.as_deref().unwrap_or(&stuck.model);
        let sup = supervisor(store, &stuck);
        let sup_name = sup.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| "Hệ thống".into());
        let sup_id = sup.as_ref().map(|s| s.id.clone()).unwrap_or_else(|| "system".into());
        let backup = store.with(|s| s.settings.fallback_model.trim().to_string());
        let backup = (!backup.is_empty() && backup != current_model).then_some(backup);

        // On policy refusal, only retry once on a DIFFERENT model (backup or colleague on another model);
        // retrying on the exact same model just wastes tokens and loops.
        if is_refusal_err(&err) {
            if attempt >= 1 {
                return r;
            }
            let diff_colleague = store
                .staff(stuck.role)
                .into_iter()
                .find(|e| e.id != stuck.id && e.id != original && e.provider != ProviderId::Mock && e.model != current_model);
            if let Some(c) = diff_colleague {
                store.post_message(
                    Some(&job.project_id),
                    &sup_id,
                    "all",
                    format!("{} gặp từ chối từ model ({}). {} giao lại cho {} (model {}).", stuck.name, short(&err, 90), sup_name, c.name, c.model),
                    "chat",
                );
                job.employee_id = c.id;
                model = None;
                continue;
            } else if let Some(b) = backup {
                store.post_message(
                    Some(&job.project_id),
                    &sup_id,
                    "all",
                    format!("{} gặp từ chối từ model ({}). {} thử lại 1 lần bằng model dự phòng {}.", stuck.name, short(&err, 90), sup_name, b),
                    "chat",
                );
                model = Some(b);
                continue;
            } else {
                return r;
            }
        }

        if attempt == 0 {
            let colleague = store.staff(stuck.role).into_iter().find(|e| e.id != stuck.id && e.id != original && e.provider != ProviderId::Mock);
            let note = if let Some(c) = colleague {
                let n = format!("{} gặp sự cố ({}). {} giao lại việc cho {}.", stuck.name, short(&err, 90), sup_name, c.name);
                job.employee_id = c.id;
                if err.contains("bước mà chưa xong") && !job.task.contains("BÀN GIAO HARNESS") {
                    job.task.push_str("\n\n[BÀN GIAO HARNESS]: Đồng nghiệp trước đã lưu bản Checkpoint tiến độ vào docs/PROGRESS.md (và mục BỘ NHỚ TIẾN ĐỘ DỰ ÁN bên dưới). Hãy nối tiếp đúng chỗ đang dở, KHÔNG làm lại hoặc đọc lại các tệp đã hoàn tất.");
                }
                n
            } else if let Some(b) = backup.clone() {
                let n = format!("{} gặp sự cố ({}). {} cho {} làm tiếp bằng model dự phòng {}.", stuck.name, short(&err, 90), sup_name, stuck.name, b);
                model = Some(b);
                n
            } else {
                String::new() // nobody else and no backup model: go straight to rewording
            };
            if !note.is_empty() {
                store.post_message(Some(&job.project_id), &sup_id, "all", note, "chat");
                continue;
            }
        }
        if model.is_none() {
            model = backup;
        }
        let Some(sup) = sup else { return r };
        store.set_runtime(&sup.id, EmpStatus::Thinking, format!("Diễn đạt lại việc cho {}", stuck.name), Some(&job.project_id), job.ticket_id.as_deref());
        let reworded = rephrase(store, &sup, &stuck, &job, &err, cancel).await;
        store.set_runtime(&sup.id, EmpStatus::Idle, "", None, None);
        match reworded {
            Some(t) => {
                job.task = t;
                store.post_message(Some(&job.project_id), &sup.id, "all", format!("{} vướng ({}). {} đã diễn đạt lại yêu cầu rõ hơn và giao lại.", stuck.name, short(&err, 90), sup.name), "chat");
            }
            None => return r,
        }
    }
    Err("aborted".into())
}

async fn run_tracked(store: &S, job: Job, model: Option<&str>, cancel: &CancellationToken) -> Result<JobResult, String> {
    let emp_id = job.employee_id.clone();
    let pid = job.project_id.clone();
    let started = now_ms();
    let r = run_job(store, job, model, cancel).await;
    let took = now_ms().saturating_sub(started);
    match &r {
        Ok(_) => store.kpi(&emp_id, |k| {
            k.jobs += 1;
            k.work_ms += took;
        }),
        Err(e) if e != "aborted" => store.kpi(&emp_id, |k| {
            k.errors += 1;
            k.work_ms += took;
        }),
        _ => {}
    }
    // Back to idle once the job ends, whatever the outcome.
    let status = if matches!(&r, Err(e) if e != "aborted") { EmpStatus::Blocked } else { EmpStatus::Idle };
    let note = match &r {
        Err(e) if e != "aborted" => short(e, 120),
        _ => String::new(),
    };
    store.set_runtime(&emp_id, status, note, Some(&pid), None);
    r
}

async fn run_job(store: &S, job: Job, model: Option<&str>, cancel: &CancellationToken) -> Result<JobResult, String> {
    let emp = store.employee(&job.employee_id).ok_or("Nhân viên không tồn tại")?;
    let project = store.project(&job.project_id).ok_or("Dự án không tồn tại")?;
    let dir = store.dir_of(&project);
    tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;

    // One person does one job at a time.
    let lock = store.emp_lock(&emp.id);
    let _guard = lock.lock().await;

    let ticket = job.ticket_id.as_deref().and_then(|id| store.ticket(id));
    let meta = Meta {
        phase: job.phase,
        project_id: project.id.clone(),
        extra: job.case_id.as_deref().and_then(|c| store.case(c)).map(|c| case_transcript(store, &c)).unwrap_or_default(),
        project_name: project.name.clone(),
        request: project.request.clone(),
        ticket: ticket.as_ref().map(|t| llm::TicketMeta { key: t.key.clone(), title: t.title.clone(), bounces: t.bounces }),
    };
    let mut tools = tools::tools_for(emp.role, job.phase);
    let mut system = system_prompt(&emp, &project, &dir);
    // Skills (role playbooks + per-employee custom skills + optional tools-mode GEMINI.md) and MCP servers.
    let root = store.data_dir.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let settings_snapshot = store.with(|s| s.settings.clone());
    system.push_str(&crate::skills::for_employee_with_settings(&root, &emp, &settings_snapshot));
    // Skill library (design guides etc.): listed by name, read in full on demand with read_skill.
    let library = crate::skills::library_prompt(&root, emp.role, job.phase);
    if !library.is_empty() && !crate::mcp::light_phase(job.phase) {
        system.push_str(&library);
        tools.push(crate::tools::read_skill_tool());
    }
    let mcp_overrides = store.with(|s| s.settings.mcp.clone());
    let mut mcp = if emp.provider == ProviderId::Mock {
        None
    } else {
        Some(crate::mcp::Session::start(&root, &dir, emp.role, job.phase, &mcp_overrides).await)
    };
    if let Some(m) = &mcp {
        for t in &m.tools {
            tools.push(llm::ToolDef { name: t.name.clone(), description: t.description.clone(), schema: t.schema.clone() });
        }
        if !m.hints.is_empty() {
            let preview_hint = if crate::workflow::has_preview(&dir) || job.phase == "design" {
                format!(
                    "\nBản xem thử sản phẩm: {} (đã build thì là bản build trong dist/; tệp lẻ như design/mockup.html mở bằng đường dẫn tương ứng).",
                    crate::config::preview_url(&project.id)
                )
            } else {
                String::new()
            };
            system.push_str(&format!("\n\nCông cụ MCP bạn có (tên dạng <nhóm>__<công cụ>):\n{}{preview_hint}", m.hints.join("\n")));
        }
    }
    let mut first = job.task.clone();
    if crate::harness::wants_briefing(job.phase) {
        let brief = crate::harness::briefing(store, &project, &dir).await;
        if !brief.is_empty() {
            first.push_str(&brief);
        }
    }
    if let Some(inbox) = take_inbox(store, &emp.id, &project.id) {
        first.push_str(&inbox);
    }
    let mut messages = vec![LlmMessage::User { text: Some(first), tool_results: vec![] }];
    let mut ctx = Ctx {
        store: store.clone(),
        project_id: project.id.clone(),
        employee: emp.clone(),
        dir: dir.clone(),
        ticket_id: job.ticket_id.clone(),
        case_id: job.case_id.clone(),
        cancel: cancel.clone(),
        outcome: None,
        created_tickets: 0,
        last_write: None,
        read_ranges: std::collections::HashMap::new(),
        read_files: std::collections::HashMap::new(),
        phase: job.phase,
    };
    let pid = Some(project.id.as_str());
    let tid = job.ticket_id.as_deref();
    let max_steps = store.with(|s| s.settings.max_agent_steps).max(3);
    let mut nudges = 0;
    let mut next_compact_step: u32 = 48;

    for step in 0..max_steps {
        if cancel.is_cancelled() {
            return Err("aborted".into());
        }
        budget_gate(store, &project.id, &emp.id, cancel).await?;
        // Pick up model changes made in the Staff panel while the job runs.
        let mut emp_now = store.employee(&emp.id).unwrap_or_else(|| emp.clone());
        if let Some(m) = model {
            emp_now.model = m.to_string();
        }
        // Harness context compaction: periodically summarize older middle turns into dense Working Memory.
        if step >= next_compact_step && messages.len() >= 65 {
            store.set_runtime(&emp.id, EmpStatus::Thinking, "Đang nén bộ nhớ làm việc (Harness)", pid, tid);
            let permit = tokio::select! {
                p = store.llm_slots.acquire() => p.map_err(|e| e.to_string())?,
                _ = cancel.cancelled() => return Err("aborted".into()),
            };
            let _ = crate::harness::maybe_compact(store, &emp_now, &project, &mut messages, 26, cancel).await;
            drop(permit);
            next_compact_step = step + 22;
        }
        store.set_runtime(&emp.id, EmpStatus::Thinking, format!("Đang suy nghĩ · {}", emp_now.model), pid, tid);
        let permit = tokio::select! {
            p = store.llm_slots.acquire() => p.map_err(|e| e.to_string())?,
            _ = cancel.cancelled() => return Err("aborted".into()),
        };
        let res = llm::chat(
            store,
            LlmRequest { employee: &emp_now, system: &system, messages: &messages, tools: &tools, max_tokens: 32000, meta: &meta },
            cancel,
        )
        .await;
        drop(permit);
        let res = match res {
            Ok(r) => r,
            Err(e) => {
                if e != "aborted" {
                    store.log_activity(&emp.id, pid, "error", &e);
                }
                return Err(e);
            }
        };
        store.add_usage(&emp.id, pid, res.input_tokens, res.output_tokens);
        if !res.text.trim().is_empty() {
            store.log_activity(&emp.id, pid, "llm", &res.text);
        }
        if res.stop == Stop::Refusal || (res.tool_calls.is_empty() && looks_like_refusal(&res.text)) {
            let msg = if res.text.trim().is_empty() {
                format!("Model {} từ chối yêu cầu này", emp_now.model)
            } else {
                format!("Model {} từ chối yêu cầu này: {}", emp_now.model, short(res.text.trim(), 160))
            };
            store.log_activity(&emp.id, pid, "error", &msg);
            return Err(msg);
        }
        messages.push(LlmMessage::Assistant {
            text: res.text.clone(),
            tool_calls: res.tool_calls.clone(),
            raw: res.raw.clone(),
            raw_provider: res.raw_provider,
        });

        if res.tool_calls.is_empty() {
            let note = if res.stop == Stop::MaxTokens {
                "Câu trả lời bị cắt vì quá dài. Hãy tiếp tục, chia nhỏ tệp nếu cần."
            } else {
                nudges += 1;
                if nudges > 2 {
                    if res.text.trim().is_empty() {
                        return Err(format!("{} không trả lời và không gọi công cụ sau nhiều lần nhắc", emp.name));
                    }
                    if matches!(job.phase, "review" | "test") {
                        return Err(format!(
                            "{} không gọi công cụ kết thúc ({}) sau nhiều lần nhắc",
                            emp.name,
                            if job.phase == "review" { "approve_code / request_changes" } else { "pass_ticket / fail_ticket" }
                        ));
                    }
                    if job.phase == "implement" {
                        return Err(format!("{} không gọi finish sau nhiều lần nhắc", emp.name));
                    }
                    // The model keeps answering in prose on non-code phases; accept that as the summary.
                    return Ok(JobResult { outcome: Outcome::Finish(short(&res.text, 1500)), created_tickets: ctx.created_tickets });
                }
                if res.text.contains("call:default_api:") {
                    "LỖI ĐỊNH DẠNG: bạn vừa in chữ 'call:default_api:...' vào văn bản thay vì gọi tool thật. ĐỪNG in chữ đó; hãy gọi trực tiếp tool kết thúc / thao tác bằng cơ chế function calling."
                } else {
                    "Hãy tiếp tục bằng tool. Nếu đã xong, gọi tool kết thúc với tóm tắt."
                }
            };
            messages.push(LlmMessage::User { text: Some(note.into()), tool_results: vec![] });
            continue;
        }

        if res.stop == Stop::MaxTokens {
            // The reply hit the output limit, so tool arguments (e.g. a file's content) are cut off.
            // Don't write half a file: make the model redo it shorter.
            store.log_activity(&emp.id, pid, "error", "Câu trả lời bị cắt vì quá dài, yêu cầu viết lại ngắn hơn");
            let results = res
                .tool_calls
                .iter()
                .map(|c| ToolResult {
                    images: vec![],
                    id: c.id.clone(),
                    content: "KHÔNG thực hiện: câu trả lời của bạn bị cắt vì vượt giới hạn độ dài, nội dung không đầy đủ. Hãy làm lại ngắn gọn hơn, hoặc chia thành nhiều tệp / nhiều lần ghi nhỏ.".into(),
                    is_error: true,
                })
                .collect();
            messages.push(LlmMessage::User { text: None, tool_results: results });
            continue;
        }

        let mut results = vec![];
        for c in &res.tool_calls {
            let label = describe_call(&c.name, &c.input);
            store.set_runtime(&emp.id, EmpStatus::Working, label.clone(), pid, tid);
            store.log_activity(&emp.id, pid, "tool", &label);
            if let Some(m) = mcp.as_mut().filter(|m| m.has(&c.name)) {
                let mut r = tokio::select! {
                    r = m.call(&c.name, &c.input) => r,
                    _ = cancel.cancelled() => return Err("aborted".into()),
                };
                // A screenshot saved to a file comes back as text only: attach the saved image so the model can see it.
                if c.name.ends_with("browser_take_screenshot") && r.images.is_empty() && !r.is_error {
                    if let Some(img) = saved_screenshot(&ctx.dir, &r.text, &c.input) {
                        r.images.push(img);
                    }
                }
                let note = if r.images.is_empty() { short(&r.text, 800).to_string() } else { format!("{} [{} ảnh]", short(&r.text, 600), r.images.len()) };
                store.log_activity(&emp.id, pid, if r.is_error { "error" } else { "result" }, note);
                results.push(ToolResult { id: c.id.clone(), content: r.text, is_error: r.is_error, images: r.images });
                continue;
            }
            // Reading an image file: show the picture instead of binary garbage.
            if c.name == "read_file" {
                let rel = c.input.get("path").and_then(Value::as_str).unwrap_or_default();
                if let Some(img) = image_mime(rel).and_then(|_| tools::safe_path(&ctx.dir, rel).ok()).and_then(|p| image_attachment(&p)) {
                    let content = format!("Ảnh {rel} (đính kèm bên dưới để bạn xem).");
                    store.log_activity(&emp.id, pid, "result", format!("{content} [1 ảnh]"));
                    results.push(ToolResult { id: c.id.clone(), content, is_error: false, images: vec![img] });
                    continue;
                }
            }
            let (content, is_error) = tools::execute(&mut ctx, &c.name, &c.input).await;
            if content == "aborted" {
                return Err("aborted".into());
            }
            store.log_activity(&emp.id, pid, if is_error { "error" } else { "result" }, short(&content, 800));
            results.push(ToolResult { id: c.id.clone(), content, is_error, images: vec![] });
        }
        let step_num = step + 1;
        let left = max_steps.saturating_sub(step_num);
        let budget_hint = if max_steps >= 20 && step_num == (max_steps * 60 / 100) {
            Some(format!(
                "[Lưu ý tiến độ: đã dùng {step_num}/{max_steps} bước. Hãy chuyển sang sửa/ghi mã nguồn và chạy test ngay, tránh đọc lặp lại các tệp đã xem.]"
            ))
        } else if max_steps >= 15 && left == 10 {
            Some(format!(
                "[CẢNH BÁO TIẾN ĐỘ: chỉ còn {left} bước ({step_num}/{max_steps})! Hãy hoàn tất thay đổi cốt lõi, chạy test kiểm chứng và gọi tool kết thúc NGAY.]"
            ))
        } else if max_steps >= 15 && left == 4 {
            Some(format!(
                "[KHẨN CẤP: chỉ còn {left} bước ({step_num}/{max_steps})! DỪNG đọc thêm tệp và gọi công cụ kết thúc (finish / approve_code / pass_ticket / fail_ticket) NGAY BÂY GIỜ với bằng chứng bạn đã có!]"
            ))
        } else {
            None
        };
        // Steering (OpenClaw): messages from the chairman / colleagues that arrived while working
        // are delivered right after the current tool batch, so the employee can change course mid-task.
        let steer = if ctx.outcome.is_none() { take_inbox(store, &emp.id, &project.id) } else { None };
        if steer.is_some() {
            store.log_activity(&emp.id, pid, "tool", "Nhận tin nhắn mới giữa chừng, cân nhắc điều chỉnh");
        }
        let steer = steer.map(|m| format!("{m}
(Tin nhắn vừa đến trong lúc bạn đang làm. Đọc kỹ và điều chỉnh hướng làm nếu cần; tin của Chủ tịch được ưu tiên hơn kế hoạch cũ.)"));
        let text = match (budget_hint, steer) {
            (Some(h), Some(m)) => Some(format!("{h}{m}")),
            (h, m) => h.or(m),
        };
        messages.push(LlmMessage::User { text, tool_results: results });
        prune_tool_results(&mut messages, KEEP_FULL_TOOL_TURNS);
        if let Some(outcome) = ctx.outcome.take() {
            return Ok(JobResult { outcome, created_tickets: ctx.created_tickets });
        }
    }
    // Before failing on step exhaustion, save a structured Harness handoff checkpoint so the next worker continues immediately.
    if !cancel.is_cancelled() {
        store.set_runtime(&emp.id, EmpStatus::Thinking, "Lưu checkpoint bàn giao (Harness)", pid, tid);
        let _ = crate::harness::save_handoff_checkpoint(store, &emp, &project, &dir, tid, &messages, cancel).await;
    }
    Err(format!("{} vượt quá {max_steps} bước mà chưa xong việc", emp.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0x00, 0x10]), "//4AEA==");
    }

    #[test]
    fn attaches_saved_screenshots() {
        let dir = std::env::temp_dir().join(format!("aico-shot-{}", crate::store::new_id()));
        std::fs::create_dir_all(dir.join("design/shots")).unwrap();
        std::fs::write(dir.join("design/shots/a.png"), b"PNG").unwrap();
        // Path printed by Playwright (escaped Windows separators) wins over the argument.
        let text = "await page.screenshot({\n  path: 'design\\\\shots\\\\a.png',\n  type: 'png' });";
        assert_eq!(saved_screenshot(&dir, text, &serde_json::json!({})), Some(("image/png".into(), "UE5H".into())));
        assert_eq!(saved_screenshot(&dir, "ok", &serde_json::json!({ "filename": "design/shots/a.png" })).map(|i| i.0), Some("image/png".into()));
        assert!(saved_screenshot(&dir, "ok", &serde_json::json!({ "filename": "../x.png" })).is_none());
        assert!(saved_screenshot(&dir, "ok", &serde_json::json!({ "filename": "notes.txt" })).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prunes_only_old_tool_results() {
        let big = "x".repeat(10_000);
        let tr = |c: &str| LlmMessage::User { text: None, tool_results: vec![ToolResult { id: "1".into(), content: c.into(), is_error: false, images: vec![("image/png".into(), "AA".into())] }] };
        let mut m = vec![tr(&big), tr(&big), tr(&big), tr(&big), LlmMessage::User { text: Some("hi".into()), tool_results: vec![] }];
        prune_tool_results(&mut m, 3);
        let len = |m: &LlmMessage| match m { LlmMessage::User { tool_results, .. } => tool_results[0].content.chars().count(), _ => 0 };
        assert!(len(&m[0]) < 2200, "oldest pruned");
        assert!(matches!(&m[0], LlmMessage::User { tool_results, .. } if tool_results[0].images.is_empty()));
        assert_eq!(len(&m[1]), 10_000);
        assert_eq!(len(&m[3]), 10_000);
        prune_tool_results(&mut m, 3); // idempotent
        assert!(len(&m[0]) < 2200);
    }

    #[test]
    fn detects_text_refusals_without_false_positives() {
        assert!(looks_like_refusal("Sorry, I cannot fulfill this request. I cannot build or test offer_claimer.py..."));
        assert!(looks_like_refusal("  I'm sorry, I can't assist with injecting Frida scripts."));
        assert!(looks_like_refusal("Tôi xin lỗi, tôi không thể hỗ trợ can thiệp hệ thống thanh toán."));
        assert!(!looks_like_refusal("Đã hoàn thành ticket T-1. Toàn bộ unit test đã xanh."));
        assert!(!looks_like_refusal("Báo cáo: nếu hệ thống từ chối kết nối thì thử lại sau 5 giây."));
        assert!(!looks_like_refusal(""));
    }
}
