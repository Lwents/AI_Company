//! The chairman chats with an employee. The employee answers right away (even when idle) and can act on
//! simple orders: let the team decide alone (no more approvals), change how many design options to make,
//! or turn feedback on a finished project into a change request. Messages also stay in the employee's inbox,
//! so whoever is working on the project picks them up mid-task.
use crate::config;
use crate::llm::{self, LlmMessage, LlmRequest, Meta, ToolDef};
use crate::store::S;
use crate::types::*;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

pub fn spawn_reply(store: &S, msg: ChatMsg) {
    let store = store.clone();
    tokio::spawn(async move {
        if let Err(e) = reply(&store, &msg).await {
            eprintln!("Trả lời tin nhắn thất bại: {e}");
            store.post_message(msg.project_id.as_deref(), &msg.to, "chairman", "Em đã nhận tin nhắn nhưng chưa trả lời được (lỗi kết nối model), em sẽ xử lý khi làm việc tiếp.", "chat");
        }
    });
}

fn reply_tool() -> ToolDef {
    ToolDef {
        name: "reply".into(),
        description: "Trả lời Chủ tịch và (nếu Chủ tịch yêu cầu) thực hiện thay đổi. Luôn gọi đúng một lần.".into(),
        schema: json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Câu trả lời gửi Chủ tịch: tiếng Việt, ngắn gọn, đúng vai của bạn; nói rõ bạn sẽ làm gì / đã làm gì." },
                "autopilot": { "type": "string", "enum": ["none", "on", "off"], "description": "on: Chủ tịch giao cho đội TỰ QUYẾT mọi câu hỏi và duyệt (không hỏi Chủ tịch nữa); off: Chủ tịch muốn duyệt lại như cũ; none: không nhắc tới." },
                "design_options": { "type": "integer", "description": "Số phương án / mẫu thiết kế Chủ tịch muốn được xem để chọn (1–8). 0 = không nhắc tới." },
                "change_request": { "type": "string", "description": "CHỈ khi dự án đã làm xong / đang chờ Chủ tịch duyệt và Chủ tịch yêu cầu sửa / thêm: mô tả đầy đủ yêu cầu thay đổi để cả đội làm lại. Bỏ trống nếu không." }
            },
            "required": ["text"]
        }),
    }
}

/// Context the employee needs to answer: who they are, the project's state, the recent conversation.
fn context(store: &S, emp: &Employee, msg: &ChatMsg) -> String {
    let mut out = String::new();
    if let Some(p) = msg.project_id.as_deref().and_then(|id| store.project(id)) {
        let pending: Vec<String> = store.with(|s| {
            s.approvals.iter().filter(|a| a.project_id.as_deref() == Some(&p.id) && a.status == ApprovalStatus::Pending).map(|a| format!("- {}", a.title)).collect()
        });
        out += &format!(
            "Dự án: \"{}\" — bước {:?}, trạng thái {:?}. Yêu cầu: {}\nChế độ tự quyết: {}. Số mẫu thiết kế cho Chủ tịch chọn: {}.\n",
            p.name,
            p.phase,
            p.status,
            short(&p.request, 400),
            if crate::workflow::autopilot(store, Some(&p.id)) { "BẬT (đội tự quyết, không hỏi Chủ tịch)" } else { "tắt" },
            crate::workflow::design_option_count(store, &p)
        );
        if !pending.is_empty() {
            out += &format!("Đang chờ Chủ tịch duyệt:\n{}\n", pending.join("\n"));
        }
    } else {
        out += &format!("Tin nhắn chung (không thuộc dự án nào). Chế độ tự quyết toàn công ty: {}.\n", if crate::workflow::autopilot(store, None) { "BẬT" } else { "tắt" });
    }
    if let Some(rt) = store.with(|s| s.runtime.get(&emp.id).cloned()).filter(|r| r.status != EmpStatus::Idle) {
        out += &format!("Bạn đang: {}.\n", rt.note);
    }
    let history: Vec<String> = store.with(|s| {
        let mut h: Vec<&ChatMsg> = s
            .messages
            .iter()
            .filter(|m| m.id != msg.id && m.project_id == msg.project_id && (m.from == emp.id || m.to == emp.id || m.to == "all" || m.to == "chairman"))
            .collect();
        let skip = h.len().saturating_sub(12);
        h.drain(..skip);
        h.iter().map(|m| format!("[{} → {}] {}", store.display_name(&m.from), store.display_name(&m.to), short(&m.text, 300))).collect()
    });
    if !history.is_empty() {
        out += &format!("\nTrao đổi gần đây:\n{}\n", history.join("\n"));
    }
    out
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() > n { s.chars().take(n).collect::<String>() + "…" } else { s.to_string() }
}

async fn reply(store: &S, msg: &ChatMsg) -> Result<(), String> {
    let emp = store.employee(&msg.to).ok_or("Không có nhân viên")?;
    let idle = store.with(|s| s.runtime.get(&emp.id).is_none_or(|r| r.status == EmpStatus::Idle));
    if idle {
        store.set_runtime(&emp.id, EmpStatus::Thinking, "Đang trả lời Chủ tịch", msg.project_id.as_deref(), None);
    }
    let system = format!(
        "Bạn là {}, {} của AI Company. {}\nChủ tịch (khách hàng, người quyết định cao nhất) vừa nhắn tin cho bạn. Trả lời lịch sự, ngắn gọn, đúng vai, xưng \"em\" và gọi \"anh/chị\". Nếu là chỉ đạo thì xác nhận và nói rõ sẽ làm gì; tin nhắn này cũng được chuyển tới người đang làm việc trong dự án. Gọi công cụ reply đúng một lần.",
        emp.name,
        config::role_label(emp.role),
        config::role_duty(emp.role)
    );
    let ask = format!("{}\nTin nhắn của Chủ tịch:\n{}", context(store, &emp, msg), msg.text);
    let meta = Meta { phase: "chat", project_id: msg.project_id.clone().unwrap_or_default(), extra: String::new(), project_name: String::new(), request: String::new(), ticket: None };
    let messages = vec![LlmMessage::User { text: Some(ask), tool_results: vec![] }];
    let tools = [reply_tool()];
    let res = llm::chat(store, LlmRequest { employee: &emp, system: &system, messages: &messages, tools: &tools, max_tokens: 4000, meta: &meta }, &CancellationToken::new()).await;
    if idle {
        store.set_runtime(&emp.id, EmpStatus::Idle, "", None, None);
    }
    let res = res?;
    store.add_usage(&emp.id, msg.project_id.as_deref(), res.input_tokens, res.output_tokens);
    let input = res.tool_calls.iter().find(|c| c.name == "reply").map(|c| c.input.clone()).unwrap_or_else(|| json!({ "text": res.text }));
    let mut text = input.get("text").and_then(Value::as_str).unwrap_or_default().trim().to_string();
    if text.is_empty() {
        text = "Dạ em đã nhận được tin nhắn.".into();
    }
    let notes = apply(store, msg.project_id.as_deref(), &input);
    let body = if notes.is_empty() { text } else { format!("{text}\n\n{}", notes.join("\n")) };
    store.post_message(msg.project_id.as_deref(), &emp.id, "chairman", body, "chat");
    Ok(())
}

/// Carries out the orders the employee recognised; returns what was done (shown to the chairman).
fn apply(store: &S, project_id: Option<&str>, input: &Value) -> Vec<String> {
    let mut done = vec![];
    match input.get("autopilot").and_then(Value::as_str) {
        Some(mode @ ("on" | "off")) => {
            let on = mode == "on";
            crate::workflow::set_autopilot(store, project_id, on);
            done.push(if on {
                let n = crate::workflow::release_pending(store, project_id);
                format!("✅ Đã BẬT chế độ tự quyết{}: đội tự quyết định, không chờ Chủ tịch duyệt nữa{}.", scope(project_id), if n > 0 { format!(" (đã tự xử lý {n} mục đang chờ)") } else { String::new() })
            } else {
                format!("✅ Đã TẮT chế độ tự quyết{}: các câu hỏi và bản thiết kế sẽ chờ Chủ tịch duyệt như cũ.", scope(project_id))
            });
        }
        _ => {}
    }
    if let Some(n) = input.get("design_options").and_then(Value::as_u64).filter(|n| (1..=8).contains(n)) {
        crate::workflow::set_design_options(store, project_id, n as u8);
        done.push(format!("✅ Thiết kế{}: Designer sẽ làm {n} mẫu cho Chủ tịch chọn.", scope(project_id)));
    }
    if let (Some(pid), Some(fb)) = (project_id, input.get("change_request").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())) {
        match crate::workflow::request_changes(store, pid, fb) {
            Ok(()) => done.push(format!("✅ Đã chuyển yêu cầu sửa cho cả đội, dự án bắt đầu vòng sửa đổi mới: {}", short(fb, 200))),
            Err(e) => done.push(format!("ℹ️ Chưa mở vòng sửa đổi: {e}. Yêu cầu đã được ghi lại, người đang làm sẽ nhận được.")),
        }
    }
    done
}

fn scope(project_id: Option<&str>) -> &'static str {
    if project_id.is_some() { " cho dự án này" } else { " cho toàn công ty" }
}
