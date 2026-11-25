//! The company process. Phases run in order; a phase whose role is not on staff is skipped.
//!
//! Plan (CEO) → Requirements (BA) → Planning (PM) → Design (Designer) → Architecture (Tech Lead)
//! → Build (Dev ⇄ Reviewer ⇄ Tester, PM standup) → Release (DevOps; bugs go back to Build)
//! → Report (CEO) → Chairman approval. The Secretary writes minutes of the kickoff and closing meetings.
use crate::agent::{self, Job};
use crate::store::{now_ms, S};
use crate::tools::Outcome;
use crate::types::*;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const ABORTED: &str = "aborted";
const MAX_RELEASE_ROUNDS: u32 = 2;

pub(crate) fn handoff(store: &S, pid: &str, from: &str, to: &str, text: impl Into<String>) {
    let text = text.into();
    store.emit(ServerEvent::Handoff { from: from.into(), to: to.into(), text: text.chars().take(60).collect() });
    store.post_message(Some(pid), from, to, text, "handoff");
}

pub(crate) fn meeting(store: &S, pid: &str, participants: Vec<String>, topic: &str, active: bool) {
    store.emit(ServerEvent::Meeting { project_id: pid.into(), participants, topic: topic.into(), active });
}

pub(crate) fn first(store: &S, roles: &[Role]) -> Option<Employee> {
    roles.iter().find_map(|r| store.staff(*r).into_iter().next())
}

fn everyone(store: &S) -> Vec<String> {
    store.with(|s| s.employees.iter().filter(|e| e.enabled).map(|e| e.id.clone()).collect())
}

pub(crate) fn summary_of(o: &Outcome) -> String {
    match o {
        Outcome::Finish(s) | Outcome::Pass(s) | Outcome::Fail(s) | Outcome::Approve(s) | Outcome::Changes(s) => s.clone(),
    }
}

/// Start (or resume) a project in the background from its current phase.
pub fn start(store: &S, pid: &str) {
    if store.cancels.lock().unwrap().contains_key(pid) {
        return;
    }
    let token = CancellationToken::new();
    store.cancels.lock().unwrap().insert(pid.to_string(), token.clone());
    store.update_project(pid, |p| {
        p.status = ProjectStatus::Running;
        p.error.clear();
    });
    let store = store.clone();
    let pid = pid.to_string();
    tokio::spawn(async move {
        let r = run_project(&store, &pid, &token).await;
        store.cancels.lock().unwrap().remove(&pid);
        if let Err(e) = r {
            if e != ABORTED && !token.is_cancelled() {
                store.update_project(&pid, |p| {
                    p.status = ProjectStatus::Failed;
                    p.error = e.clone();
                });
                store.post_message(Some(&pid), "system", "chairman", format!("Dự án gặp lỗi: {e}"), "system");
            }
        }
        let ids: Vec<String> = store.with(|s| {
            s.runtime.iter().filter(|(_, r)| r.project_id.as_deref() == Some(&pid)).map(|(k, _)| k.clone()).collect()
        });
        for id in ids {
            store.set_runtime(&id, EmpStatus::Idle, "", None, None);
        }
        meeting(&store, &pid, vec![], "", false);
    });
}

/// Pause or stop: cancels running jobs (including live operations); the project can be resumed later.
pub fn halt(store: &S, pid: &str, status: ProjectStatus) {
    store.update_project(pid, |p| p.status = status);
    if let Some(t) = store.cancels.lock().unwrap().get(pid) {
        t.cancel();
    }
    store.stop_ops(pid);
}

async fn run_project(store: &S, pid: &str, cancel: &CancellationToken) -> Result<(), String> {
    loop {
        if cancel.is_cancelled() {
            return Err(ABORTED.into());
        }
        let p = store.project(pid).ok_or("Dự án không tồn tại")?;
        let next = match p.phase {
            Phase::Plan => {
                phase_plan(store, &p, cancel).await?;
                Phase::Requirements
            }
            Phase::Requirements => {
                phase_requirements(store, &p, cancel).await?;
                Phase::Planning
            }
            Phase::Planning => {
                phase_planning(store, &p, cancel).await?;
                Phase::Design
            }
            Phase::Design => {
                phase_design(store, &p, cancel).await?;
                Phase::Architecture
            }
            Phase::Architecture => {
                phase_architecture(store, &p, cancel).await?;
                Phase::Build
            }
            Phase::Build => {
                phase_build(store, &p, cancel).await?;
                Phase::Release
            }
            Phase::Release => {
                if phase_release(store, &p, cancel).await? {
                    Phase::Build
                } else {
                    Phase::Report
                }
            }
            Phase::Report => {
                phase_report(store, &p, cancel).await?;
                store.update_project(pid, |p| {
                    p.phase = Phase::AwaitingApproval;
                    p.status = ProjectStatus::AwaitingApproval;
                });
                return Ok(());
            }
            Phase::Launch => {
                phase_launch(store, &p, cancel).await?;
                store.update_project(pid, |p| {
                    p.phase = Phase::Operating;
                    p.status = ProjectStatus::Operating;
                });
                return Ok(());
            }
            Phase::AwaitingApproval | Phase::Operating | Phase::Done => return Ok(()),
        };
        store.update_project(pid, |p| p.phase = next);
    }
}

fn revision_note(p: &Project) -> String {
    match p.feedback.last() {
        Some(fb) => format!(
            "\n\nĐÂY LÀ VÒNG SỬA ĐỔI {}. Chủ tịch yêu cầu thay đổi:\n{fb}\nSản phẩm và tài liệu cũ đã có trong thư mục dự án; chỉ làm phần cần thay đổi.",
            p.feedback.len()
        ),
        None => String::new(),
    }
}

fn ticket_list(store: &S, pid: &str) -> String {
    let mut s = String::new();
    for t in store.tickets_of(pid) {
        s.push_str(&format!("- {} [{:?}, ưu tiên {}] {}: {}\n", t.key, t.status, t.priority, t.title, t.description));
    }
    if s.is_empty() {
        s.push_str("(chưa có ticket)\n");
    }
    s
}

/// The Secretary writes minutes of a meeting (if the company has one).
pub(crate) async fn minutes(store: &S, p: &Project, topic: &str, source: &str, cancel: &CancellationToken) -> Result<(), String> {
    let Some(sec) = first(store, &[Role::Secretary]) else { return Ok(()) };
    let n = store.update_project(&p.id, |p| {
        p.meetings += 1;
        p.meetings
    });
    let slug = if topic.contains("khởi động") {
        "khoi-dong"
    } else if topic.contains("ra mắt") {
        "ra-mat"
    } else {
        "tong-ket"
    };
    let file = format!("docs/meetings/{:02}-{slug}.md", n.unwrap_or(1));
    let task = format!(
        "Bạn vừa dự cuộc họp \"{topic}\" của dự án \"{}\". Đọc {source} rồi viết biên bản họp vào {file}: thời gian, thành phần tham dự, nội dung chính, quyết định, phân công, việc tiếp theo. Ngắn gọn, gạch đầu dòng. Xong gọi finish.",
        p.name
    );
    agent::run(store, Job { employee_id: sec.id.clone(), project_id: p.id.clone(), phase: "minutes", ticket_id: None, case_id: None, task }, cancel).await?;
    store.post_message(Some(&p.id), &sec.id, "all", format!("Đã gửi biên bản {topic}: {file}"), "chat");
    Ok(())
}

async fn phase_plan(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let ceo = first(store, &[Role::Ceo]).ok_or("Công ty cần ít nhất 1 CEO đang làm việc")?;
    let all = everyone(store);
    store.post_message(Some(&p.id), "chairman", &ceo.id, if p.feedback.is_empty() { p.request.clone() } else { p.feedback.last().cloned().unwrap_or_default() }, "chat");
    meeting(store, &p.id, all.clone(), "Họp khởi động", true);
    let task = format!(
        "Chủ tịch giao dự án \"{}\":\n{}{}\n\nViệc của bạn: phân tích yêu cầu và viết kế hoạch ngắn gọn vào docs/PLAN.md (mục tiêu, phạm vi, công nghệ gợi ý — ưu tiên thứ đơn giản chạy ngay, các hạng mục chính, ai trong công ty làm gì). Chỉ hỏi Chủ tịch (ask_chairman) khi thật sự không thể tự quyết. Xong gọi finish.",
        p.name,
        p.request,
        revision_note(p)
    );
    let r = agent::run(store, Job { employee_id: ceo.id.clone(), project_id: p.id.clone(), phase: "plan", ticket_id: None, case_id: None, task }, cancel).await;
    let r = match r {
        Ok(r) => r,
        Err(e) => {
            meeting(store, &p.id, all, "", false);
            return Err(e);
        }
    };
    let minutes_result = minutes(store, p, "Họp khởi động", "docs/PLAN.md", cancel).await;
    meeting(store, &p.id, all, "", false);
    minutes_result?;
    let next = first(store, &[Role::Ba, Role::Pm]).unwrap_or(ceo.clone());
    if next.id != ceo.id {
        handoff(store, &p.id, &ceo.id, &next.id, format!("Kế hoạch đã xong (docs/PLAN.md). {}", summary_of(&r.outcome)));
    }
    Ok(())
}

async fn phase_requirements(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let ba = first(store, &[Role::Ba, Role::Pm, Role::Ceo]).ok_or("Không có ai viết yêu cầu (BA/PM/CEO)")?;
    let task = format!(
        "Đọc docs/PLAN.md. Viết docs/REQUIREMENTS.md gồm user story và tiêu chí nghiệm thu. Sau đó tạo ticket bằng create_ticket: 2–6 ticket nhỏ, làm được độc lập, theo thứ tự nên làm, mỗi ticket có tiêu chí nghiệm thu kiểm tra được. Xong gọi finish.{}",
        revision_note(p)
    );
    let r = agent::run(store, Job { employee_id: ba.id.clone(), project_id: p.id.clone(), phase: "requirements", ticket_id: None, case_id: None, task }, cancel).await?;
    if r.created_tickets == 0 {
        // Make sure there is always something to build.
        let seq = store.update_project(&p.id, |p| {
            p.ticket_seq += 1;
            p.ticket_seq
        });
        store.insert_ticket(Ticket {
            id: crate::store::new_id(),
            project_id: p.id.clone(),
            key: format!("T-{}", seq.unwrap_or(1)),
            title: if p.feedback.is_empty() { "Hoàn thành yêu cầu dự án".into() } else { "Thực hiện yêu cầu sửa đổi".into() },
            description: p.feedback.last().cloned().unwrap_or_else(|| p.request.clone()),
            acceptance: vec!["Đáp ứng mô tả".into()],
            kind: "task".into(),
            status: TicketStatus::Todo,
            assignee: None,
            created_by: ba.id.clone(),
            bounces: 0,
            notes: vec![],
            priority: 2,
            created_at: now_ms(),
            updated_at: now_ms(),
        });
    }
    Ok(())
}

/// PM orders the backlog (only when a separate BA wrote the requirements).
async fn phase_planning(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let Some(pm) = first(store, &[Role::Pm]) else { return Ok(()) };
    let Some(ba) = first(store, &[Role::Ba]) else { return Ok(()) };
    handoff(store, &p.id, &ba.id, &pm.id, "Ticket đã có trên Kanban, nhờ PM lên kế hoạch sprint.");
    let task = format!(
        "Bạn là PM. Danh sách ticket hiện có:\n{}\nĐọc docs/PLAN.md và docs/REQUIREMENTS.md. Đặt độ ưu tiên cho từng ticket bằng set_priority (1 = làm trước). Viết docs/SPRINT.md: mục tiêu sprint, thứ tự làm, phụ thuộc giữa các ticket, rủi ro. Nếu thiếu việc quan trọng thì tạo thêm ticket. Xong gọi finish.{}",
        ticket_list(store, &p.id),
        revision_note(p)
    );
    agent::run(store, Job { employee_id: pm.id.clone(), project_id: p.id.clone(), phase: "planning", ticket_id: None, case_id: None, task }, cancel).await?;
    Ok(())
}

async fn phase_design(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let Some(designer) = first(store, &[Role::Designer]) else { return Ok(()) };
    if let Some(from) = first(store, &[Role::Pm, Role::Ba, Role::Ceo]) {
        handoff(store, &p.id, &from.id, &designer.id, "Yêu cầu đã chốt, nhờ thiết kế giao diện.");
    }
    let task = format!(
        "Đọc docs/PLAN.md và docs/REQUIREMENTS.md. Viết docs/DESIGN.md: phong cách, bảng màu (mã hex), font, khoảng cách, bố cục từng màn hình, các thành phần giao diện và trạng thái (hover, lỗi, trống). Tạo design/tokens.css chứa biến CSS (màu, font, bo góc, khoảng cách) để Dev dùng lại. Xong gọi finish.{}",
        revision_note(p)
    );
    agent::run(store, Job { employee_id: designer.id.clone(), project_id: p.id.clone(), phase: "design", ticket_id: None, case_id: None, task }, cancel).await?;
    Ok(())
}

async fn phase_architecture(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let Some(lead) = first(store, &[Role::Architect]) else { return Ok(()) };
    if let Some(from) = first(store, &[Role::Designer, Role::Pm, Role::Ba, Role::Ceo]) {
        handoff(store, &p.id, &from.id, &lead.id, "Yêu cầu và thiết kế đã sẵn sàng, nhờ anh/chị thiết kế kiến trúc.");
    }
    let task = format!(
        "Đọc docs/PLAN.md, docs/REQUIREMENTS.md (và docs/DESIGN.md nếu có). Viết docs/ARCHITECTURE.md: công nghệ, cấu trúc thư mục, quy ước code, cách chạy. Có thể tạo thêm ticket kỹ thuật (vd. khởi tạo dự án) nếu cần. Xong gọi finish.{}",
        revision_note(p)
    );
    agent::run(store, Job { employee_id: lead.id.clone(), project_id: p.id.clone(), phase: "architecture", ticket_id: None, case_id: None, task }, cancel).await?;
    Ok(())
}

fn is_open(s: TicketStatus) -> bool {
    matches!(s, TicketStatus::Todo | TicketStatus::InProgress | TicketStatus::Review | TicketStatus::Testing)
}

async fn phase_build(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let devs = store.staff(Role::Dev);
    if devs.is_empty() {
        return Err("Công ty cần ít nhất 1 Dev đang làm việc".into());
    }
    // Work interrupted by a pause starts over from To Do.
    for t in store.tickets_of(&p.id) {
        if matches!(t.status, TicketStatus::InProgress | TicketStatus::Review | TicketStatus::Testing) {
            store.update_ticket(&t.id, |t| t.status = TicketStatus::Todo);
        }
    }
    let todo = store.tickets_of(&p.id).iter().filter(|t| t.status == TicketStatus::Todo).count();

    // Daily standup led by the PM.
    if let Some(pm) = first(store, &[Role::Pm]) {
        let mut team: Vec<String> = vec![pm.id.clone()];
        for r in [Role::Dev, Role::Reviewer, Role::Tester, Role::Designer] {
            team.extend(store.staff(r).into_iter().map(|e| e.id));
        }
        meeting(store, &p.id, team.clone(), "Họp daily", true);
        let task = format!(
            "Họp daily đầu đợt làm việc. Các ticket:\n{}\nNhắn từng Dev (message_colleague, to_role = dev) việc cần làm trước và lưu ý quan trọng. Ngắn gọn. Xong gọi finish với tóm tắt cuộc họp.",
            ticket_list(store, &p.id)
        );
        let r = agent::run(store, Job { employee_id: pm.id.clone(), project_id: p.id.clone(), phase: "standup", ticket_id: None, case_id: None, task }, cancel).await;
        meeting(store, &p.id, team, "", false);
        let r = r?;
        store.post_message(Some(&p.id), &pm.id, "all", format!("Daily: {}", summary_of(&r.outcome)), "chat");
    } else if let Some(from) = first(store, &[Role::Architect, Role::Designer, Role::Ba, Role::Ceo]) {
        for d in &devs {
            handoff(store, &p.id, &from.id, &d.id, format!("Có {todo} ticket cần làm trên bảng Kanban."));
        }
    }

    run_dev_workers(store, &p.id, cancel).await
}

/// All devs work through the open tickets of a project in parallel until none are left.
pub(crate) async fn run_dev_workers(store: &S, pid: &str, cancel: &CancellationToken) -> Result<(), String> {
    let devs = store.staff(Role::Dev);
    if devs.is_empty() {
        return Err("Công ty cần ít nhất 1 Dev đang làm việc".into());
    }
    let mut handles = vec![];
    for d in devs {
        let store = store.clone();
        let pid = pid.to_string();
        let cancel = cancel.clone();
        handles.push(tokio::spawn(async move { dev_worker(&store, &pid, &d.id, &cancel).await }));
    }
    let mut first_err = None;
    for h in handles {
        match h.await {
            // A failing worker does not stop colleagues; they finish their own tickets first.
            Ok(Ok(())) => {}
            Ok(Err(e)) => first_err = first_err.or(Some(e)),
            Err(e) => first_err = first_err.or(Some(e.to_string())),
        }
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

async fn dev_worker(store: &S, pid: &str, dev_id: &str, cancel: &CancellationToken) -> Result<(), String> {
    loop {
        if cancel.is_cancelled() {
            return Err(ABORTED.into());
        }
        // My own returned tickets first, then by priority, then oldest.
        let picked = store.with(|s| {
            let mut cands: Vec<&Ticket> = s
                .tickets
                .iter()
                .filter(|t| t.project_id == pid && t.status == TicketStatus::Todo)
                .filter(|t| t.assignee.as_deref().map_or(true, |a| a == dev_id || !s.employees.iter().any(|e| e.id == a && e.enabled)))
                .collect();
            cands.sort_by_key(|t| (t.assignee.as_deref() != Some(dev_id), t.priority, t.created_at));
            cands.first().map(|t| t.id.clone())
        });
        match picked {
            Some(tid) => {
                store.update_ticket(&tid, |t| {
                    t.status = TicketStatus::InProgress;
                    t.assignee = Some(dev_id.to_string());
                });
                process_ticket(store, pid, dev_id, &tid, cancel).await?;
            }
            None => {
                let open = store.tickets_of(pid).iter().any(|t| is_open(t.status));
                if !open {
                    return Ok(());
                }
                store.set_runtime(dev_id, EmpStatus::Idle, "Chờ ticket", Some(pid), None);
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(800)) => {}
                    _ = cancel.cancelled() => return Err(ABORTED.into()),
                }
            }
        }
    }
}

fn ticket_brief(t: &Ticket) -> String {
    let mut s = format!(
        "Ticket {}: {}\nLoại: {} · Ưu tiên: {}\nMô tả: {}\nTiêu chí nghiệm thu:\n",
        t.key, t.title, t.kind, t.priority, t.description
    );
    for a in &t.acceptance {
        s.push_str(&format!("- {a}\n"));
    }
    if !t.notes.is_empty() {
        s.push_str("\nLịch sử / phản hồi trước đó:\n");
        for n in t.notes.iter().rev().take(6).rev() {
            s.push_str(&format!("- {n}\n"));
        }
    }
    s
}

async fn process_ticket(store: &S, pid: &str, dev_id: &str, tid: &str, cancel: &CancellationToken) -> Result<(), String> {
    let max_bounces = store.with(|s| s.settings.max_bounces);
    loop {
        let t = store.ticket(tid).ok_or("Ticket không tồn tại")?;
        let task = format!(
            "{}\nĐọc docs/REQUIREMENTS.md (và docs/DESIGN.md, design/tokens.css, docs/ARCHITECTURE.md nếu có) cùng code hiện có. Viết code hoàn thành ticket này, theo thiết kế nếu có, tự kiểm tra bằng run_command nếu phù hợp, rồi gọi finish với tóm tắt thay đổi.",
            ticket_brief(&t)
        );
        let r = agent::run(store, Job { employee_id: dev_id.into(), project_id: pid.into(), phase: "implement", ticket_id: Some(tid.into()), case_id: None, task }, cancel).await?;
        let dev_summary = summary_of(&r.outcome);
        let dev_name = store.employee(dev_id).map(|e| e.name).unwrap_or_default();
        store.update_ticket(tid, |t| t.notes.push(format!("{dev_name} (Dev): {dev_summary}")));

        // Optional code review.
        if let Some(rev) = first(store, &[Role::Reviewer]) {
            store.update_ticket(tid, |t| t.status = TicketStatus::Review);
            handoff(store, pid, dev_id, &rev.id, format!("Nhờ review {}: {}", t.key, t.title));
            let task = format!("{}\nDev báo: {dev_summary}\n\nReview phần code liên quan. Gọi approve_code nếu đạt, request_changes nếu có lỗi thật cần sửa.", ticket_brief(&t));
            let r = agent::run(store, Job { employee_id: rev.id.clone(), project_id: pid.into(), phase: "review", ticket_id: Some(tid.into()), case_id: None, task }, cancel).await?;
            store.kpi(&rev.id, |k| k.reviews += 1);
            if let Outcome::Changes(c) = &r.outcome {
                store.kpi(&rev.id, |k| k.changes_requested += 1);
                store.kpi(dev_id, |k| k.tickets_returned += 1);
                handoff(store, pid, &rev.id, dev_id, format!("{} cần sửa: {c}", t.key));
                if bounce(store, tid, format!("{} (Reviewer): {c}", rev.name), max_bounces) {
                    return Ok(());
                }
                continue;
            }
            let note = format!("{} (Reviewer): Duyệt. {}", rev.name, summary_of(&r.outcome));
            store.update_ticket(tid, |t| t.notes.push(note));
        }

        // Optional QA.
        let Some(tester) = first(store, &[Role::Tester]) else {
            store.update_ticket(tid, |t| t.status = TicketStatus::Done);
            store.kpi(dev_id, |k| k.tickets_done += 1);
            return Ok(());
        };
        store.update_ticket(tid, |t| t.status = TicketStatus::Testing);
        handoff(store, pid, dev_id, &tester.id, format!("Nhờ test {}: {}", t.key, t.title));
        let task = format!(
            "{}\nDev báo: {dev_summary}\n\nKiểm tra ticket theo từng tiêu chí nghiệm thu: đọc code liên quan, chạy lệnh kiểm tra/test nếu có thể (có thể viết test trong tests/). Gọi pass_ticket nếu đạt hết, fail_ticket kèm mô tả lỗi cụ thể nếu chưa đạt.",
            ticket_brief(&t)
        );
        let r = agent::run(store, Job { employee_id: tester.id.clone(), project_id: pid.into(), phase: "test", ticket_id: Some(tid.into()), case_id: None, task }, cancel).await?;
        match r.outcome {
            Outcome::Fail(report) => {
                store.kpi(&tester.id, |k| k.bugs_found += 1);
                store.kpi(dev_id, |k| k.tickets_returned += 1);
                handoff(store, pid, &tester.id, dev_id, format!("{} chưa đạt: {report}", t.key));
                if bounce(store, tid, format!("{} (Tester): {report}", tester.name), max_bounces) {
                    return Ok(());
                }
            }
            other => {
                store.update_ticket(tid, |t| {
                    t.status = TicketStatus::Done;
                    t.notes.push(format!("{} (Tester): Đạt. {}", tester.name, summary_of(&other)));
                });
                store.kpi(dev_id, |k| k.tickets_done += 1);
                store.post_message(Some(pid), &tester.id, dev_id, format!("{} đạt ✔", t.key), "chat");
                return Ok(());
            }
        }
    }
}

/// Send a ticket back to the dev. Returns true if it is now blocked (too many rounds).
fn bounce(store: &S, tid: &str, note: String, max: u32) -> bool {
    store
        .update_ticket(tid, |t| {
            t.notes.push(note);
            t.bounces += 1;
            if t.bounces > max {
                t.status = TicketStatus::Blocked;
                true
            } else {
                t.status = TicketStatus::InProgress;
                false
            }
        })
        .unwrap_or(true)
}

/// DevOps checks the build. Returns true when bugs were filed and the team must build again.
async fn phase_release(store: &S, p: &Project, cancel: &CancellationToken) -> Result<bool, String> {
    release_check(store, p, p.release_rounds < MAX_RELEASE_ROUNDS, cancel).await
}

pub(crate) async fn release_check(store: &S, p: &Project, allow_bugs: bool, cancel: &CancellationToken) -> Result<bool, String> {
    let Some(ops) = first(store, &[Role::Devops]) else { return Ok(false) };
    if let Some(from) = first(store, &[Role::Tester, Role::Dev]) {
        handoff(store, &p.id, &from.id, &ops.id, "Các ticket đã xong, nhờ DevOps kiểm tra bản phát hành.");
    }
    let task = format!(
        "Kiểm tra sản phẩm có chạy được không: xem các tệp, chạy lệnh build/test phù hợp (vd. node --check, npm install && npm run build, python -m py_compile, cargo build). Viết hoặc cập nhật README.md ở gốc dự án: giới thiệu, yêu cầu, cách cài, cách chạy. Ghi docs/RELEASE.md: đã kiểm tra gì, kết quả.{} Xong gọi finish.{}",
        if allow_bugs {
            " Nếu có lỗi chặn phát hành, gọi report_issue cho từng lỗi (Dev sẽ sửa rồi bạn kiểm tra lại)."
        } else {
            " Đây là lần kiểm tra cuối: ghi lỗi còn lại vào docs/RELEASE.md, không mở ticket mới."
        },
        revision_note(p)
    );
    let r = agent::run(store, Job { employee_id: ops.id.clone(), project_id: p.id.clone(), phase: "release", ticket_id: None, case_id: None, task }, cancel).await?;
    if r.created_tickets > 0 && allow_bugs {
        store.update_project(&p.id, |p| p.release_rounds += 1);
        if let Some(dev) = first(store, &[Role::Dev]) {
            handoff(store, &p.id, &ops.id, &dev.id, format!("Bản build có {} lỗi chặn phát hành, nhờ sửa.", r.created_tickets));
        }
        return Ok(true);
    }
    store.update_project(&p.id, |p| p.release_rounds = 0);
    store.post_message(Some(&p.id), &ops.id, "all", format!("Phát hành: {}", summary_of(&r.outcome)), "chat");
    Ok(false)
}

async fn phase_report(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let ceo = first(store, &[Role::Ceo]).ok_or("Công ty cần ít nhất 1 CEO đang làm việc")?;
    let tickets = store.tickets_of(&p.id);
    let mut list = String::new();
    for t in &tickets {
        list.push_str(&format!("- {} [{:?}] {} (sửa lại {} lần)\n", t.key, t.status, t.title, t.bounces));
    }
    if let Some(from) = first(store, &[Role::Devops, Role::Tester, Role::Dev]) {
        handoff(store, &p.id, &from.id, &ceo.id, "Sản phẩm đã sẵn sàng, mời CEO tổng kết.");
    }
    let all = everyone(store);
    meeting(store, &p.id, all.clone(), "Họp tổng kết", true);
    let task = format!(
        "Đội đã xử lý các ticket:\n{list}\nĐọc docs/ (PLAN, REQUIREMENTS, SPRINT, DESIGN, RELEASE nếu có) và xem các tệp sản phẩm. Viết docs/REPORT.md: đã làm gì, cách chạy/mở sản phẩm, hạn chế còn lại (nêu rõ ticket bị Blocked nếu có), ghi nhận thành viên nổi bật. Sau đó gọi finish với lời báo cáo ngắn gửi Chủ tịch.{}",
        revision_note(p)
    );
    let r = agent::run(store, Job { employee_id: ceo.id.clone(), project_id: p.id.clone(), phase: "report", ticket_id: None, case_id: None, task }, cancel).await;
    let r = match r {
        Ok(r) => r,
        Err(e) => {
            meeting(store, &p.id, all, "", false);
            return Err(e);
        }
    };
    let minutes_result = minutes(store, p, "Họp tổng kết", "docs/REPORT.md", cancel).await;
    meeting(store, &p.id, all, "", false);
    minutes_result?;
    let summary = summary_of(&r.outcome);
    store.update_project(&p.id, |p| p.report = summary.clone());
    store.post_message(Some(&p.id), &ceo.id, "chairman", summary, "report");
    store.emit(ServerEvent::Handoff { from: ceo.id.clone(), to: "chairman".into(), text: "Báo cáo dự án".into() });
    Ok(())
}

/// After the chairman approves: every business department prepares the launch, in parallel.
async fn phase_launch(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let ceo = first(store, &[Role::Ceo]);
    let pid = p.id.clone();
    let portal = format!("/portal/{pid}/");
    let jobs: Vec<(Role, &'static str, String)> = vec![
        (Role::Legal, "launch_legal", "Soạn legal/TERMS.md (điều khoản sử dụng) và legal/PRIVACY.md (chính sách bảo mật) cho sản phẩm, dựa trên README.md và docs/. Ngắn gọn, dễ hiểu, phù hợp pháp luật Việt Nam. Ghi các rủi ro pháp lý cần lưu ý (kể cả với nội dung quảng cáo) vào legal/NOTES.md. Xong gọi finish.".to_string()),
        (Role::Marketing, "launch_marketing", format!("Sản phẩm sắp ra mắt. Đọc README.md, docs/REQUIREMENTS.md, docs/DESIGN.md (nếu có). Tạo bộ ra mắt:\n- marketing/landing.html: trang giới thiệu một trang, tự chứa CSS, có nút \"Dùng thử\" trỏ tới ../index.html và nút \"Liên hệ\" trỏ tới {portal}\n- marketing/social-posts.md: 5 bài cho Facebook / TikTok / Zalo\n- marketing/email.md: email thông báo ra mắt\n- marketing/SEO.md: từ khóa, tiêu đề, mô tả\nNội dung trung thực, không hứa tính năng không có. Xong gọi finish.")),
        (Role::Sales, "launch_sales", "Đọc README.md và docs/. Viết sales/pricing.md (2–3 gói, giá VNĐ hợp lý cho thị trường Việt Nam, gói dùng thử nếu phù hợp), sales/pitch.md (kịch bản tư vấn, câu hỏi khai thác nhu cầu, xử lý từ chối), sales/faq.md. Xong gọi finish.".to_string()),
        (Role::Support, "launch_support", "Đọc README.md và docs/. Viết support/FAQ.md (10 câu hỏi thường gặp kèm câu trả lời) và support/playbook.md (quy trình chăm sóc khách hàng: phân loại yêu cầu, thời gian phản hồi, khi nào chuyển đội kỹ thuật, mẫu câu trả lời). Xong gọi finish.".to_string()),
        (Role::Accountant, "launch_finance", "Gọi company_finance để lấy số liệu thật. Viết finance/report.md (chi phí AI theo nhân viên và dự án, doanh thu, lãi lỗ, khoản tốn kém nhất, đề xuất tiết kiệm) và finance/budget.md (dự toán chi phí vận hành tháng tới). Xong gọi finish.".to_string()),
        (Role::Hr, "launch_hr", "Gọi company_kpi. Viết hr/review.md: đánh giá từng nhân viên sau dự án (điểm mạnh, điểm cần cải thiện, đề xuất đổi model / đào tạo). Chỉ gọi propose_hire khi số liệu cho thấy thật sự thiếu người. Xong gọi finish.".to_string()),
    ];
    let mut team: Vec<String> = ceo.iter().map(|e| e.id.clone()).collect();
    for (role, _, _) in &jobs {
        team.extend(store.staff(*role).into_iter().map(|e| e.id));
    }
    team.extend(store.staff(Role::Secretary).into_iter().map(|e| e.id));
    meeting(store, &pid, team.clone(), "Họp ra mắt sản phẩm", true);
    if let Some(ceo) = &ceo {
        store.post_message(Some(&pid), &ceo.id, "all", format!("Chủ tịch đã duyệt \"{}\". Các phòng ban chuẩn bị ra mắt!", p.name), "chat");
    }

    let mut handles = vec![];
    for (role, phase, task) in jobs {
        let Some(emp) = first(store, &[role]) else { continue };
        let store = store.clone();
        let pid = pid.clone();
        let cancel = cancel.clone();
        handles.push(tokio::spawn(async move {
            let r = agent::run(&store, Job { employee_id: emp.id.clone(), project_id: pid, phase, ticket_id: None, case_id: None, task }, &cancel).await;
            if r.is_ok() {
                store.kpi(&emp.id, |k| k.campaigns += 1);
            }
            r.map(|_| ())
        }));
    }
    let mut first_err = None;
    for h in handles {
        match h.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => first_err = first_err.or(Some(e)),
            Err(e) => first_err = first_err.or(Some(e.to_string())),
        }
    }
    if let Some(e) = first_err {
        meeting(store, &pid, team, "", false);
        return Err(e);
    }
    let m = minutes(store, p, "Họp ra mắt sản phẩm", "README.md, marketing/, sales/pricing.md, support/FAQ.md", cancel).await;
    meeting(store, &pid, team, "", false);
    m?;
    if let Some(ceo) = &ceo {
        store.post_message(
            Some(&pid),
            &ceo.id,
            "chairman",
            format!("Sản phẩm \"{}\" đã ra mắt và bắt đầu vận hành. Cổng khách hàng: {portal} — CSKH và Sales trực 24/7, lỗi khách báo sẽ được Dev sửa và phát hành tự động.", p.name),
            "report",
        );
        store.emit(ServerEvent::Handoff { from: ceo.id.clone(), to: "chairman".into(), text: "Đã ra mắt sản phẩm!".into() });
    }
    Ok(())
}
