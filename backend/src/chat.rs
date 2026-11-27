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
                "autopilot": { "type": "string", "enum": ["none", "on", "off"], "description": "Mặc định none. on: CHỈ khi Chủ tịch nói rõ giao cho đội TỰ QUYẾT / không cần duyệt nữa; off: CHỈ khi Chủ tịch nói rõ muốn duyệt lại; none: mọi trường hợp khác." },
                "design_options": { "type": "integer", "description": "Số phương án / mẫu thiết kế Chủ tịch muốn được xem để chọn (1–8). 0 = không nhắc tới." },
                "task": { "type": "string", "description": "CHỈ khi Chủ tịch GIAO VIỆC cụ thể và đã RÕ (viết, sửa, thiết kế, kiểm tra…): mô tả đầy đủ việc cần làm và kết quả mong muốn; bạn bắt đầu làm ngay. Nếu bạn còn hỏi lại / xin xác nhận / đưa phương án cho Chủ tịch chọn thì BỎ TRỐNG (chờ Chủ tịch trả lời rồi mới nhận việc)." },
                "task_project": { "type": "string", "description": "Việc làm ở đâu: mã dự án trong danh sách dự án của công ty; \"new\" nếu việc thuộc một sản phẩm KHÁC chưa có dự án (vd. đang ở dự án đồng hồ mà Chủ tịch bảo thiết kế web cờ vua) — hệ thống tạo dự án nháp riêng; bỏ trống = dự án đang chọn." },
                "new_project_name": { "type": "string", "description": "Tên ngắn cho dự án nháp khi task_project là \"new\" (vd. \"Web cờ vua\")." },
                "choose_design": { "type": "string", "description": "CHỈ khi đang chờ Chủ tịch chọn phương án thiết kế và Chủ tịch nói chọn phương án nào: chữ cái của phương án (a, b, c…), kèm góp ý nếu có, ví dụ \"b: chữ to hơn\". Bỏ trống nếu không." },
                "change_request": { "type": "string", "description": "CHỈ khi Chủ tịch yêu cầu sửa / thêm / làm lại cho dự án (đã xong, đang chờ duyệt hoặc đang tạm dừng): mô tả đầy đủ yêu cầu thay đổi để cả đội làm vòng sửa đổi. Bỏ trống nếu không." }
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
            s.approvals
                .iter()
                .filter(|a| a.project_id.as_deref() == Some(&p.id) && a.status == ApprovalStatus::Pending)
                .map(|a| {
                    let opts: Vec<String> = a.options.iter().map(|o| format!("{}. {}", o.id.to_uppercase(), o.title)).collect();
                    if opts.is_empty() { format!("- {}", a.title) } else { format!("- {} (các phương án: {})", a.title, opts.join("; ")) }
                })
                .collect()
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
        if !design_ahead(p.phase) {
            out += "Bước thiết kế của dự án này ĐÃ XONG (đã chốt thiết kế). Đổi số mẫu không làm lại thiết kế: muốn có mẫu mới thì Chủ tịch phải yêu cầu sửa (vòng sửa đổi). Đừng hứa sẽ làm mẫu ngay.
";
        }
        if !pending.is_empty() {
            out += &format!("Đang chờ Chủ tịch duyệt:\n{}\n", pending.join("\n"));
        }
    } else {
        out += &format!("Tin nhắn chung (không thuộc dự án nào). Chế độ tự quyết toàn công ty: {}.\n", if crate::workflow::autopilot(store, None) { "BẬT" } else { "tắt" });
    }
    match store.with(|s| s.runtime.get(&emp.id).cloned()).filter(|r| r.status != EmpStatus::Idle) {
        Some(rt) => out += &format!("Bạn đang: {}.\n", rt.note),
        None => out += "Bạn đang rảnh, nhận việc được ngay.\n",
    }
    let projects: Vec<String> = store.with(|s| s.projects.iter().rev().take(15).map(|p| format!("- {} (mã {}): {}", p.name, p.id, p.request.chars().take(80).collect::<String>())).collect());
    out += &format!("Các dự án của công ty (mới nhất trước):\n{}\n", projects.join("\n"));
    if let Some(p) = msg.project_id.as_deref().and_then(|id| store.project(id)).filter(|p| !p.team.is_empty()) {
        out += if p.team.contains(&emp.id) { "Bạn thuộc nhóm làm dự án này.\n" } else { "Bạn KHÔNG thuộc nhóm làm dự án này (vẫn nhận việc Chủ tịch giao được).\n" };
    }
    // Copy the messages out first: display_name locks the store again (the lock isn't re-entrant).
    let recent: Vec<ChatMsg> = store.with(|s| {
        let h: Vec<ChatMsg> = s
            .messages
            .iter()
            .filter(|m| m.id != msg.id && m.project_id == msg.project_id && (m.from == emp.id || m.to == emp.id || m.to == "all" || m.to == "chairman"))
            .cloned()
            .collect();
        let skip = h.len().saturating_sub(12);
        h.into_iter().skip(skip).collect()
    });
    let history: Vec<String> = recent.iter().map(|m| format!("[{} → {}] {}", store.display_name(&m.from), store.display_name(&m.to), short(&m.text, 300))).collect();
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
    let notes = apply(store, &emp, msg.project_id.as_deref(), &input, &msg.text);
    let body = if notes.is_empty() { text } else { format!("{text}\n\n{}", notes.join("\n")) };
    store.post_message(msg.project_id.as_deref(), &emp.id, "chairman", body, "chat");
    Ok(())
}

/// Carries out the orders the employee recognised; returns what was done (shown to the chairman).
fn apply(store: &S, emp: &Employee, project_id: Option<&str>, input: &Value, said: &str) -> Vec<String> {
    let mut done = vec![];
    if let Some(task) = input.get("task").and_then(Value::as_str).map(str::trim).filter(|t| t.chars().count() > 3) {
        let target = input.get("task_project").and_then(Value::as_str).map(str::trim).unwrap_or_default();
        let picked = if target == "new" {
            let name = input.get("new_project_name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty()).unwrap_or("Việc riêng của Chủ tịch");
            let p = crate::workflow::draft_project(store, name, task);
            done.push(format!("🗂 Đã tạo dự án nháp \"{}\" (đang tạm dừng) để làm riêng việc này; muốn làm thành sản phẩm đầy đủ thì bấm Tiếp tục ở tab Dự án.", p.name));
            Some(p)
        } else if !target.is_empty() {
            store.project(target).or_else(|| project_id.and_then(|id| store.project(id)))
        } else {
            project_id.and_then(|id| store.project(id))
        };
        match picked {
            None => done.push("ℹ️ Anh/chị chọn một dự án (tab Dự án) rồi giao lại, để em biết làm trong thư mục nào.".into()),
            Some(p) => {
                let busy = store.with(|s| s.runtime.get(&emp.id).filter(|r| r.status != EmpStatus::Idle && r.note != "Đang trả lời Chủ tịch").map(|r| r.note.clone()));
                spawn_task(store, emp, &p.id, task);
                done.push(match busy {
                    Some(note) => format!("📝 Đã nhận việc. Em đang bận ({note}), xong việc đó em làm ngay và báo lại."),
                    None => "📝 Đã nhận việc, em bắt đầu làm ngay và sẽ báo lại khi xong.".into(),
                });
            }
        }
    }
    match input.get("autopilot").and_then(Value::as_str) {
        // Only a real change counts (models sometimes fill "off" when the chairman didn't mention it).
        Some(mode @ ("on" | "off")) if (mode == "on") != crate::workflow::autopilot(store, project_id) => {
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
        let past = project_id.and_then(|id| store.project(id)).filter(|p| !design_ahead(p.phase));
        done.push(match past {
            None => format!("✅ Thiết kế{}: Designer sẽ làm {n} mẫu cho Chủ tịch chọn.", scope(project_id)),
            Some(p) if matches!(p.phase, Phase::AwaitingApproval | Phase::Done | Phase::Operating | Phase::Launch) => format!(
                "ℹ️ Dự án này đã làm xong nên chưa làm lại thiết kế được. Đã ghi {n} mẫu cho lần thiết kế tiếp theo: anh/chị nhắn yêu cầu sửa (ví dụ \"làm lại giao diện\") là đội mở vòng sửa đổi và Designer gửi {n} mẫu để chọn."
            ),
            Some(_) => format!(
                "ℹ️ Dự án này đã qua bước thiết kế (đang làm bước sau) nên Designer chưa làm mẫu mới ngay. Đã ghi {n} mẫu: các dự án mới và vòng sửa đổi sau sẽ có {n} mẫu để chọn. Muốn làm lại giao diện ngay: tạm dừng dự án rồi nhắn yêu cầu sửa."
            ),
        });
    }
    if let (Some(pid), Some(pick)) = (project_id, input.get("choose_design").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())) {
        let pending = store.with(|s| {
            s.approvals.iter().find(|a| a.project_id.as_deref() == Some(pid) && a.status == ApprovalStatus::Pending && !a.options.is_empty()).map(|a| (a.id.clone(), a.options.clone()))
        });
        match pending {
            Some((id, opts)) => {
                let letter = pick.chars().next().map(|c| c.to_ascii_lowercase()).unwrap_or('a');
                match opts.iter().find(|o| o.id.starts_with(letter)) {
                    Some(o) => {
                        let note = pick[pick.char_indices().nth(1).map(|(i, _)| i).unwrap_or(pick.len())..].trim_start_matches([':', '-', ',', '.', ' ']).trim();
                        // The model may give only the letter: keep the chairman's own words as feedback then.
                        let note = if note.is_empty() && said.trim().chars().count() > 12 { said.trim() } else { note };
                        let answer = format!("Chọn phương án {} — {}", o.id.to_uppercase(), o.title) + &if note.is_empty() { String::new() } else { format!("
Góp ý: {note}") };
                        store.resolve_approval(&id, true, Some(answer));
                        done.push(format!("✅ Đã chọn phương án {} \"{}\", Designer hoàn thiện thiết kế theo phương án này.", o.id.to_uppercase(), o.title));
                    }
                    None => done.push(format!("ℹ️ Không có phương án \"{letter}\"; anh/chị chọn trong {}.", opts.iter().map(|o| o.id.to_uppercase()).collect::<Vec<_>>().join(", "))),
                }
            }
            None => done.push("ℹ️ Hiện không có phương án thiết kế nào đang chờ chọn.".into()),
        }
    }
    if let (Some(pid), Some(fb)) = (project_id, input.get("change_request").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())) {
        match crate::workflow::request_changes(store, pid, fb) {
            Ok(()) => done.push(format!("✅ Đã chuyển yêu cầu sửa cho cả đội, dự án bắt đầu vòng sửa đổi mới: {}", short(fb, 200))),
            Err(e) => done.push(format!("ℹ️ Chưa mở vòng sửa đổi: {e}. Yêu cầu đã được ghi lại, người đang làm sẽ nhận được.")),
        }
    }
    done
}

/// The employee does a job the chairman gave them in chat, then reports back. Project work has
/// priority: if they are needed there, the side task pauses and resumes once they are free again.
fn spawn_task(store: &S, emp: &Employee, project_id: &str, task: &str) {
    let store = store.clone();
    let emp_id = emp.id.clone();
    let pid = project_id.to_string();
    let task = task.to_string();
    let designer = emp.role == Role::Designer;
    // A designer's ideas go to their own folder; several of them come back as a choice card.
    let ideas = format!("design/ideas/{}", crate::store::now_ms() / 1000);
    tokio::spawn(async move {
        for round in 0..6 {
            let stop = CancellationToken::new();
            store.side_jobs.lock().unwrap().insert(emp_id.clone(), stop.clone());
            let intro = if round == 0 {
                "Chủ tịch giao việc trực tiếp cho bạn (ngoài quy trình dự án):".to_string()
            } else {
                "TIẾP TỤC việc Chủ tịch giao mà bạn đã làm dở (bị tạm dừng để làm việc chính của dự án). Xem các tệp đã làm rồi làm tiếp phần còn lại, đừng làm lại từ đầu. Việc gốc:".to_string()
            };
            let job = crate::agent::Job {
                employee_id: emp_id.clone(),
                project_id: pid.clone(),
                phase: "adhoc",
                ticket_id: None,
                case_id: None,
                task: format!(
                    "{intro}\n{task}\n\nLàm trong thư mục dự án, đúng vai trò và kỹ năng của bạn. Không làm hỏng phần đang có của dự án.{}\nXong gọi finish với tóm tắt kết quả: đã làm gì, ở tệp nào, còn gì chưa xong.",
                    if designer {
                        format!("\nNếu việc là thiết kế / phác thảo giao diện: mỗi mẫu một thư mục {ideas}/<a|b|c|…>/ gồm README.md (dòng đầu \"# <Tên mẫu>\", sau đó 2–3 câu ý tưởng), mockup.html tự chứa, và ảnh chụp bằng công cụ trình duyệt: browser_take_screenshot filename \"{ideas}/<x>/desktop.png\" (1280px, fullPage) và \"{ideas}/<x>/mobile.png\" (375px). Chủ tịch sẽ xem ảnh và chọn. Không sửa thiết kế chính của dự án (design/mockup.html).")
                    } else {
                        String::new()
                    }
                ),
            };
            let r = crate::agent::run(&store, job, &stop).await;
            store.side_jobs.lock().unwrap().remove(&emp_id);
            match r {
                Ok(r) => {
                    store.post_message(Some(&pid), &emp_id, "chairman", format!("✅ Xong việc anh/chị giao: {}", crate::workflow::summary_of(&r.outcome)), "chat");
                    if designer {
                        offer_ideas(&store, &emp_id, &pid, &ideas, &task).await;
                    }
                    return;
                }
                Err(_) if stop.is_cancelled() => {
                    store.post_message(Some(&pid), &emp_id, "chairman", "⏸ Em tạm dừng việc anh/chị giao để làm việc chính của dự án. Xong việc chính em làm tiếp ạ.", "chat");
                    wait_until_free(&store, &emp_id).await;
                    store.post_message(Some(&pid), &emp_id, "chairman", "▶ Em rảnh rồi, em làm tiếp việc anh/chị giao.", "chat");
                }
                Err(e) => {
                    store.post_message(Some(&pid), &emp_id, "chairman", format!("⚠️ Em chưa làm xong việc anh/chị giao: {e}"), "chat");
                    // Ran out of steps while polishing: the ideas already made can still be chosen from.
                    if designer {
                        offer_ideas(&store, &emp_id, &pid, &ideas, &task).await;
                    }
                    return;
                }
            }
        }
        store.post_message(Some(&pid), &emp_id, "chairman", "⚠️ Em bị việc chính gọi đi nhiều lần nên chưa xong việc anh/chị giao; anh/chị nhắn lại khi dự án bớt bận nhé.", "chat");
    });
}

/// Several design ideas from a side task: the chairman picks one from the screenshots, then the
/// designer polishes it (another side task).
async fn offer_ideas(store: &S, emp_id: &str, pid: &str, base: &str, task: &str) {
    let Some(p) = store.project(pid) else { return };
    let mut opts = crate::workflow::design_options_at(&store.dir_of(&p), base);
    if opts.len() < 2 {
        return;
    }
    crate::workflow::ensure_shots(&p, &store.dir_of(&p), &mut opts).await;
    let list: Vec<String> = opts.iter().map(|o| format!("- **{}. {}**", o.id.to_uppercase(), o.title)).collect();
    let Ok(a) = store
        .request_choice(
            Some(pid),
            emp_id,
            ApprovalKind::Question,
            format!("Chọn mẫu thiết kế: {}", task.chars().take(60).collect::<String>()),
            format!("Em làm {} mẫu theo việc anh/chị giao, mời anh/chị xem ảnh và chọn (có thể ghi thêm góp ý):\n{}", opts.len(), list.join("\n")),
            opts.clone(),
            &CancellationToken::new(),
        )
        .await
    else {
        return;
    };
    let answer = a.answer.unwrap_or_default();
    if answer.trim().is_empty() && a.status == ApprovalStatus::Denied {
        return;
    }
    let (picked, feedback) = crate::workflow::parse_choice(&answer, &opts);
    let o = &opts[picked.unwrap_or(0)];
    let Some(emp) = store.employee(emp_id) else { return };
    let fb = feedback.map(|f| format!(" Góp ý của Chủ tịch: {f}")).unwrap_or_default();
    spawn_task(
        store,
        &emp,
        pid,
        &format!("Chủ tịch chọn mẫu {} \"{}\" (thư mục {}/{}/).{fb} Hoàn thiện mẫu này: làm đầy đủ các màn hình chính và trạng thái quan trọng, chụp lại ảnh 1280px và 375px vào chính thư mục đó.", o.id.to_uppercase(), o.title, base, o.id),
    );
}

/// A choice card answered after the step that asked for it is gone (server restarted): the designer
/// who made the options polishes the chosen one as a side task.
pub fn follow_up_choice(store: &S, a: &Approval) {
    let (Some(pid), Some(answer)) = (a.project_id.as_deref(), a.answer.as_deref()) else { return };
    let Some(emp) = store.employee(&a.employee_id) else { return };
    let (picked, feedback) = crate::workflow::parse_choice(answer, &a.options);
    let o = &a.options[picked.unwrap_or(0)];
    let folder = o.link.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let fb = feedback.map(|f| format!(" Góp ý của Chủ tịch: {f}")).unwrap_or_default();
    spawn_task(
        store,
        &emp,
        pid,
        &format!("Chủ tịch chọn mẫu {} \"{}\" (thư mục {folder}/).{fb} Hoàn thiện mẫu này: làm đầy đủ các màn hình chính và trạng thái quan trọng, chụp lại ảnh 1280px và 375px vào chính thư mục đó.", o.id.to_uppercase(), o.title),
    );
}

/// Waits until the employee has had no project work for a while (45 s in a row).
async fn wait_until_free(store: &S, emp_id: &str) {
    let mut quiet = 0;
    while quiet < 3 {
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        let idle = store.with(|s| s.runtime.get(emp_id).is_none_or(|r| r.status == EmpStatus::Idle));
        let free = store.emp_lock(emp_id).try_lock().is_ok();
        quiet = if idle && free { quiet + 1 } else { 0 };
    }
}

fn scope(project_id: Option<&str>) -> &'static str {
    if project_id.is_some() { " cho dự án này" } else { " cho toàn công ty" }
}

/// The project hasn't finished its design phase yet (so the number of design options still matters).
fn design_ahead(phase: Phase) -> bool {
    matches!(phase, Phase::Plan | Phase::Requirements | Phase::Planning | Phase::Design)
}

#[cfg(test)]
mod side_task_tests {
    use super::*;
    use std::time::Duration;

    fn project(id: &str) -> Project {
        Project {
            id: id.into(),
            name: "Thử".into(),
            slug: format!("thu-{id}"),
            request: "Trang thử".into(),
            feedback: vec![],
            phase: Phase::Build,
            status: ProjectStatus::Running,
            budget_usd: 100.0,
            spent_usd: 0.0,
            report: String::new(),
            error: String::new(),
            ticket_seq: 0,
            release_rounds: 0,
            design_reviews: 0,
            qa_rounds: 0,
            security_reviews: 0,
            auto_retries: 0,
            retry_at: 0,
            autopilot: false,
            design_options: 0,
            team: vec![],
            meetings: 0,
            tech: String::new(),
            versions: vec![],
            existing: false,
            source: String::new(),
            dir: String::new(),
            created_at: 0,
            updated_at: 0,
        }
    }

    /// The chairman's side task stops as soon as the project needs the same person, who reports it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn project_work_pauses_side_task() {
        let root = std::env::temp_dir().join(format!("aico-side-{}", crate::store::new_id()));
        let store = crate::store::Store::load(&root);
        store.with(|s| s.settings.mock_delay_ms = 400);
        let designer = store.staff(Role::Designer).remove(0);
        store.with(|s| s.projects.push(project("p1")));

        spawn_task(&store, &designer, "p1", "Thiết kế thử một logo");
        // Wait until the side task is actually running.
        let mut running = false;
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if store.side_jobs.lock().unwrap().contains_key(&designer.id) && store.emp_lock(&designer.id).try_lock().is_err() {
                running = true;
                break;
            }
        }
        assert!(running, "side task did not start");

        // Main project work for the same person: must not wait for the side task to finish.
        let t0 = std::time::Instant::now();
        let job = crate::agent::Job { employee_id: designer.id.clone(), project_id: "p1".into(), phase: "design_review", ticket_id: None, case_id: None, task: "Việc chính".into() };
        let main = tokio::spawn({
            let store = store.clone();
            async move { crate::agent::run(&store, job, &CancellationToken::new()).await }
        });
        // The side task is stopped and the chairman is told.
        let mut paused = false;
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            paused = store.with(|s| s.messages.iter().any(|m| m.from == designer.id && m.to == "chairman" && m.text.starts_with("⏸")));
            if paused {
                break;
            }
        }
        assert!(paused, "side task was not paused");
        assert!(t0.elapsed() < Duration::from_secs(5), "main work waited {:?}", t0.elapsed());
        main.abort();
        let _ = std::fs::remove_dir_all(&root);
    }
}
