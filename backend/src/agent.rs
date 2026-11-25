//! One AI employee doing one job: a tool-use loop until a terminal tool is called.
use crate::config;
use crate::llm::{self, LlmMessage, LlmRequest, Meta, Stop, ToolResult};
use crate::store::{now_ms, S};
use crate::tools::{self, Ctx, Outcome};
use crate::types::*;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

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
- Ưu tiên giải pháp đơn giản, chạy được ngay, ít phụ thuộc.\n\
- Không đọc/ghi ra ngoài thư mục dự án. Không đưa API key vào code.\n\
- Viết bằng tiếng Việt (code và tên tệp bằng tiếng Anh).\n\
- Khi xong việc, PHẢI gọi tool kết thúc (finish / pass_ticket / fail_ticket / approve_code / request_changes) kèm tóm tắt ngắn.",
        duty = config::role_duty(e.role),
        name = e.name,
        pname = project.name,
        dir = dir.display(),
    );
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

fn describe_call(name: &str, input: &serde_json::Value) -> String {
    let g = |k: &str| input.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    match name {
        "write_file" => format!("Ghi {}", g("path")),
        "read_file" => format!("Đọc {}", g("path")),
        "list_files" => "Xem danh sách tệp".into(),
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

pub async fn run(store: &S, job: Job, cancel: &CancellationToken) -> Result<JobResult, String> {
    let emp_id = job.employee_id.clone();
    let pid = job.project_id.clone();
    let started = now_ms();
    let r = run_job(store, job, cancel).await;
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

async fn run_job(store: &S, job: Job, cancel: &CancellationToken) -> Result<JobResult, String> {
    let emp = store.employee(&job.employee_id).ok_or("Nhân viên không tồn tại")?;
    let project = store.project(&job.project_id).ok_or("Dự án không tồn tại")?;
    let dir = store.workspace_dir.join(&project.slug);
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
    let tools = tools::tools_for(emp.role, job.phase);
    let system = system_prompt(&emp, &project, &dir);
    let mut first = job.task.clone();
    if let Some(inbox) = take_inbox(store, &emp.id, &project.id) {
        first.push_str(&inbox);
    }
    let mut messages = vec![LlmMessage::User { text: Some(first), tool_results: vec![] }];
    let mut ctx = Ctx {
        store: store.clone(),
        project_id: project.id.clone(),
        employee: emp.clone(),
        dir,
        ticket_id: job.ticket_id.clone(),
        case_id: job.case_id.clone(),
        cancel: cancel.clone(),
        outcome: None,
        created_tickets: 0,
        last_write: None,
    };
    let pid = Some(project.id.as_str());
    let tid = job.ticket_id.as_deref();
    let max_steps = store.with(|s| s.settings.max_agent_steps).max(3);
    let mut nudges = 0;

    for _step in 0..max_steps {
        if cancel.is_cancelled() {
            return Err("aborted".into());
        }
        budget_gate(store, &project.id, &emp.id, cancel).await?;
        // Pick up model changes made in the Staff panel while the job runs.
        let emp_now = store.employee(&emp.id).unwrap_or_else(|| emp.clone());
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
        if res.stop == Stop::Refusal {
            return Err(format!("Model {} từ chối yêu cầu này", emp_now.model));
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
                    // The model keeps answering in prose; accept that as the summary.
                    return Ok(JobResult { outcome: Outcome::Finish(short(&res.text, 1500)), created_tickets: ctx.created_tickets });
                }
                "Hãy tiếp tục bằng tool. Nếu đã xong, gọi tool kết thúc với tóm tắt."
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
            let (content, is_error) = tools::execute(&mut ctx, &c.name, &c.input).await;
            if content == "aborted" {
                return Err("aborted".into());
            }
            store.log_activity(&emp.id, pid, if is_error { "error" } else { "result" }, short(&content, 800));
            results.push(ToolResult { id: c.id.clone(), content, is_error });
        }
        messages.push(LlmMessage::User { text: None, tool_results: results });
        if let Some(outcome) = ctx.outcome.take() {
            return Ok(JobResult { outcome, created_tickets: ctx.created_tickets });
        }
    }
    Err(format!("{} vượt quá {max_steps} bước mà chưa xong việc", emp.name))
}
