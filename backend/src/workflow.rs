//! The company process. Phases run in order; a phase whose role is not on staff is skipped.
//!
//! Plan (CEO) → Requirements (BA) → Planning (PM) → Design (Designer) → Architecture (Tech Lead)
//! → Build (Dev ⇄ Reviewer ⇄ Tester, PM standup) → Release (DevOps; bugs go back to Build)
//! → Report (CEO) → Chairman approval. The Secretary writes minutes of the kickoff and closing meetings.
//! At kickoff the department heads discuss the CEO's draft for `settings.meeting_rounds` rounds before the CEO decides.
//!
//! The process scales with the project's size (`Project.mode`), like a real company:
//! - "fast" (S): one engineer builds it, an automatic browser check, a Tester tries it.
//! - "standard" (M): ~7 people; CEO plan without a vote, short requirements, design options, short
//!   architecture, Dev ⇄ Reviewer ⇄ Tester per ticket, ONE final QA, the browser gate, the report.
//! - "full" (L): everything above plus the debate and vote, sprint planning, constitution, data design,
//!   consistency analysis, design / security / DevOps reviews, specification, minutes and retrospective.
//! A change request never restarts the process: the PM analyses its impact on the existing backlog and
//! only the affected steps run again (design and / or architecture, then build).
use crate::agent::{self, Job};
use crate::store::{now_ms, S};
use crate::tools::Outcome;
use crate::types::*;
use std::path::Path;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// True when the project directory has a static web entry served by `/preview/<pid>/`.
pub(crate) fn has_preview(dir: &Path) -> bool {
    dir.join("index.html").is_file()
        || dir.join("dist").join("index.html").is_file()
        || dir.join("frontend").join("dist").join("index.html").is_file()
        || dir.join("frontend").join("index.html").is_file()
        || dir.join("design").join("mockup.html").is_file()
}

const ABORTED: &str = "aborted";
const MAX_RELEASE_ROUNDS: u32 = 3;
/// Whole-product QA passes per round of work (each may send bugs back to Build).
const MAX_QA_ROUNDS: u32 = 2;
/// Design reviews per round of work (each may send UI tickets back to Build).
const MAX_DESIGN_REVIEWS: u32 = 2;

pub(crate) fn handoff(store: &S, pid: &str, from: &str, to: &str, text: impl Into<String>) {
    let text = text.into();
    store.emit(ServerEvent::Handoff { from: from.into(), to: to.into(), text: text.chars().take(60).collect() });
    store.post_message(Some(pid), from, to, text, "handoff");
}

pub(crate) fn meeting(store: &S, pid: &str, participants: Vec<String>, topic: &str, active: bool) {
    store.emit(ServerEvent::Meeting { project_id: pid.into(), participants, topic: topic.into(), active });
}

tokio::task_local! {
    /// The project whose workflow is running on this task (see `start`).
    static CURRENT_PROJECT: String;
}

/// People of a role who work on the current project: its team when it has one, else everyone.
pub(crate) fn team_staff(store: &S, role: Role) -> Vec<Employee> {
    let all = store.staff(role);
    let (team, small) = CURRENT_PROJECT
        .try_with(|p| p.clone())
        .ok()
        .and_then(|pid| store.project(&pid))
        .map(|p| (p.team.clone(), is_small(&p)))
        .unwrap_or_default();
    if team.is_empty() {
        return all;
    }
    let mine: Vec<Employee> = all.iter().filter(|e| team.contains(&e.id)).cloned().collect();
    if !mine.is_empty() {
        return mine;
    }
    // A small project's team is deliberate: a role it leaves out is skipped, not borrowed from the company.
    // On a full project, someone hired for a role the team lacks still gets the work.
    if small && role != Role::Dev { vec![] } else { all }
}

pub(crate) fn first(store: &S, roles: &[Role]) -> Option<Employee> {
    roles.iter().find_map(|r| team_staff(store, *r).into_iter().next())
}

/// Picks a team for a project: per role the people least busy with other running projects
/// (one per role; developers: every free one, so they can code in parallel).
pub fn assign_team(store: &S, pid: &str) -> Vec<String> {
    let mode = store.project(pid).map(|p| p.mode).unwrap_or_default();
    // Who works on a project of this size (None = every role).
    let (keep, max_devs): (Option<&[Role]>, usize) = match mode.as_str() {
        "fast" => (Some(&[Role::Ceo, Role::Architect, Role::Dev, Role::Tester]), 1),
        "full" => (None, usize::MAX),
        _ => (Some(&[Role::Ceo, Role::Pm, Role::Designer, Role::Architect, Role::Dev, Role::Reviewer, Role::Tester]), 2),
    };
    let running: Vec<String> = store.cancels.lock().unwrap().keys().filter(|k| k.as_str() != pid).cloned().collect();
    let (emps, load) = store.with(|s| {
        let mut load: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for p in s.projects.iter().filter(|p| running.contains(&p.id)) {
            for id in &p.team {
                *load.entry(id.clone()).or_default() += 1;
            }
        }
        // The system maintainer works on AI Company itself, never on customer projects.
        (s.employees.iter().filter(|e| e.enabled && e.role != Role::Maintainer).cloned().collect::<Vec<_>>(), load)
    });
    let mut team = vec![];
    let mut roles: Vec<Role> = vec![];
    for e in &emps {
        if !roles.contains(&e.role) {
            roles.push(e.role);
        }
    }
    for role in roles {
        if keep.is_some_and(|k| !k.contains(&role)) {
            continue;
        }
        let people: Vec<&Employee> = emps.iter().filter(|e| e.role == role).collect();
        let min = people.iter().map(|e| load.get(&e.id).copied().unwrap_or(0)).min().unwrap_or(0);
        let free: Vec<&Employee> = people.into_iter().filter(|e| load.get(&e.id).copied().unwrap_or(0) == min).collect();
        let take = if role == Role::Dev { free.len().min(max_devs) } else { 1 };
        team.extend(free.into_iter().take(take).map(|e| e.id.clone()));
    }
    store.update_project(pid, |p| p.team = team.clone());
    team
}

fn everyone(store: &S) -> Vec<String> {
    store.with(|s| s.employees.iter().filter(|e| e.enabled && e.role != Role::Maintainer).map(|e| e.id.clone()).collect())
}

/// Who attends the project's meetings: its team (everyone when it has none).
fn members(store: &S, p: &Project) -> Vec<String> {
    let team = store.project(&p.id).map(|x| x.team).unwrap_or_default();
    if team.is_empty() { everyone(store) } else { team }
}

pub(crate) fn summary_of(o: &Outcome) -> String {
    match o {
        Outcome::Finish(s) | Outcome::Pass(s) | Outcome::Fail(s) | Outcome::Approve(s) | Outcome::Changes(s) => s.clone(),
    }
}

/// Start (or resume) a project in the background from its current phase.
const RETRY_DELAY: Duration = Duration::from_secs(300);

/// Restarts a failed project after `delay`, if it is still failed with a retry pending and nobody restarted it.
fn schedule_retry(store: &S, pid: &str, delay: Duration) {
    let store = store.clone();
    let pid = pid.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        let pending = store.project(&pid).is_some_and(|p| p.status == ProjectStatus::Failed && p.retry_at > 0);
        let running = store.cancels.lock().unwrap().contains_key(&pid);
        if pending && !running {
            store.update_project(&pid, |p| p.auto_retries += 1);
            store.post_message(Some(&pid), "system", "chairman", "Tự chạy lại dự án sau sự cố.", "system");
            start(&store, &pid);
        }
    });
}

/// After a server restart: re-arm the automatic restarts that were pending (their timers lived in memory).
pub fn resume_pending_retries(store: &S) {
    let now = crate::store::now_ms();
    let pending: Vec<(String, u64)> = store.with(|s| {
        s.projects.iter().filter(|p| p.status == ProjectStatus::Failed && p.retry_at > 0).map(|p| (p.id.clone(), p.retry_at)).collect()
    });
    for (id, due) in pending {
        // Overdue ones wait a little so the server is fully up first.
        let wait = Duration::from_millis(due.saturating_sub(now)).max(Duration::from_secs(10));
        schedule_retry(store, &id, wait);
    }
}

pub fn start(store: &S, pid: &str) {
    if store.cancels.lock().unwrap().contains_key(pid) {
        return;
    }
    let token = CancellationToken::new();
    store.cancels.lock().unwrap().insert(pid.to_string(), token.clone());
    store.update_project(pid, |p| {
        p.status = ProjectStatus::Running;
        p.error.clear();
        p.retry_at = 0;
    });
    if store.project(pid).is_some_and(|p| p.team.is_empty()) {
        let team = assign_team(store, pid);
        let names: Vec<String> = team.iter().map(|id| store.display_name(id)).collect();
        store.post_message(Some(pid), "system", "all", format!("Nhóm dự án: {}.", names.join(", ")), "system");
    }
    let store = store.clone();
    let pid = pid.to_string();
    tokio::spawn(async move {
        // Workflow steps pick their people from this project's team.
        let r = CURRENT_PROJECT.scope(pid.clone(), run_project(&store, &pid, &token)).await;
        store.cancels.lock().unwrap().remove(&pid);
        match r {
            Err(e) if e != ABORTED && !token.is_cancelled() => {
                let max = store.with(|s| s.settings.auto_retries);
                let tries = store
                    .update_project(&pid, |p| {
                        p.status = ProjectStatus::Failed;
                        p.error = e.clone();
                        p.auto_retries
                    })
                    .unwrap_or(max);
                if tries < max {
                    store.post_message(Some(&pid), "system", "chairman", format!("Dự án gặp sự cố: {e}. Hệ thống sẽ tự chạy lại sau 5 phút (lần {}/{max}).", tries + 1), "system");
                    let due = crate::store::now_ms() + RETRY_DELAY.as_millis() as u64;
                    store.update_project(&pid, |p| p.retry_at = due);
                    schedule_retry(&store, &pid, RETRY_DELAY);
                } else {
                    store.post_message(Some(&pid), "system", "chairman", format!("Dự án gặp lỗi: {e}. Đã tự thử lại {max} lần không được, cần Chủ tịch kiểm tra rồi bấm Tiếp tục."), "system");
                }
            }
            Ok(()) => {
                store.update_project(&pid, |p| p.auto_retries = 0);
            }
            _ => {}
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
    if let Some(p) = store.project(pid) {
        crate::gitflow::prepare(store, &p).await;
    }
    loop {
        if cancel.is_cancelled() {
            return Err(ABORTED.into());
        }
        let p = store.project(pid).ok_or("Dự án không tồn tại")?;
        let next = match p.phase {
            Phase::Plan => {
                if decide_mode(store, &p) == "fast" {
                    fast_plan(store, &p)?;
                    Phase::Build
                } else if is_revision(store, &p) {
                    // A change request: impact analysis on the existing backlog, then only the affected steps.
                    let scope = phase_change(store, &p, cancel).await?;
                    if scope.contains("design") {
                        Phase::Design
                    } else if scope.contains("arch") {
                        Phase::Architecture
                    } else {
                        Phase::Build
                    }
                } else {
                    phase_plan(store, &p, cancel).await?;
                    Phase::Requirements
                }
            }
            Phase::Requirements => {
                phase_requirements(store, &p, cancel).await?;
                // The PM's sprint plan is for big projects; on a medium one the requirements order the tickets.
                if is_standard(&p) { Phase::Design } else { Phase::Planning }
            }
            Phase::Planning => {
                phase_planning(store, &p, cancel).await?;
                Phase::Design
            }
            Phase::Design => {
                phase_design(store, &p, cancel).await?;
                // A change that only touches the look goes straight back to coding.
                if !p.change_scope.is_empty() && !p.change_scope.contains("arch") { Phase::Build } else { Phase::Architecture }
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
                // Fast lane: no design / security / QA rounds — the browser check before hand-over still applies.
                let again = if is_fast(&p) {
                    ship_gate(store, &p, cancel).await?
                } else if is_standard(&p) {
                    release_standard(store, &p, cancel).await?
                } else {
                    phase_release(store, &p, cancel).await?
                };
                if again {
                    Phase::Build
                } else {
                    Phase::Report
                }
            }
            Phase::Report => {
                phase_report(store, &p, cancel).await?;
                if !is_small(&p) {
                    retrospective(store, &p, cancel).await;
                }
                crate::gitflow::commit(store, pid, &format!("{}: bàn giao{}", p.name, if p.feedback.is_empty() { String::new() } else { format!(" sau yêu cầu sửa #{}", p.feedback.len()) })).await;
                let label = if p.feedback.is_empty() { "Bàn giao lần đầu".to_string() } else { format!("Bàn giao sau yêu cầu sửa #{}", p.feedback.len()) };
                crate::history::snapshot(store, pid, &label).await;
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
        crate::gitflow::commit(store, pid, &format!("{}: xong bước {}", p.name, phase_label(p.phase))).await;
        store.update_project(pid, |p| p.phase = next);
    }
}

fn short_text(s: &str, n: usize) -> String {
    if s.chars().count() > n { s.chars().take(n).collect::<String>() + "…" } else { s.to_string() }
}

fn phase_label(p: Phase) -> &'static str {
    match p {
        Phase::Plan => "khởi động",
        Phase::Requirements => "yêu cầu",
        Phase::Planning => "kế hoạch sprint",
        Phase::Design => "thiết kế",
        Phase::Architecture => "kiến trúc",
        Phase::Build => "lập trình",
        Phase::Release => "phát hành",
        Phase::Report => "báo cáo",
        _ => "khác",
    }
}

fn revision_note(p: &Project) -> String {
    let existing = if p.existing {
        "\n\nLƯU Ý: dự án được xây trên MÃ NGUỒN CÓ SẴN của khách hàng (xem docs/EXISTING-CODE.md). Không viết lại từ đầu: giữ công nghệ, cấu trúc và phong cách code hiện có, thay đổi tối thiểu và đúng chỗ cho yêu cầu; chuẩn cấu trúc của công ty áp dụng cho phần code mới và cải tiến dần, không đập đi xây lại. Không làm hỏng tính năng đang chạy."
    } else {
        ""
    };
    existing.to_string() + &match p.feedback.last() {
        Some(fb) => format!(
            "\n\nĐÂY LÀ VÒNG SỬA ĐỔI {}. Chủ tịch yêu cầu thay đổi:\n{fb}\nSản phẩm và tài liệu cũ đã có trong thư mục dự án; chỉ làm phần cần thay đổi.",
            p.feedback.len()
        ),
        None => String::new(),
    }
}

/// The project's non-negotiable principles (Spec-Driven Development "constitution"). Updated on revisions.
async fn write_constitution(store: &S, p: &Project, ceo: &Employee, cancel: &CancellationToken) -> Result<(), String> {
    let exists = store.dir_of(p).join("docs").join("CONSTITUTION.md").is_file();
    if exists && p.feedback.is_empty() {
        return Ok(());
    }
    let task = format!(
        "{} docs/CONSTITUTION.md — NGUYÊN TẮC BẤT DI BẤT DỊCH của dự án \"{}\", rút ra từ yêu cầu của khách, docs/TECH-CHOICE.md, docs/PLAN.md và docs/DECISIONS.md. 5–10 nguyên tắc, mỗi nguyên tắc: tên ngắn, nội dung bằng chữ PHẢI / KHÔNG ĐƯỢC, lý do một câu, cách kiểm tra là đã tuân thủ. Ví dụ loại nguyên tắc: phạm vi công nghệ, ưu tiên thiết bị, dữ liệu và quyền riêng tư, chất lượng / kiểm thử bắt buộc, hiệu năng, khả năng truy cập, cấu trúc module. Chỉ ghi điều thật sự không được vi phạm, không lặp lại kế hoạch. Mọi nhân viên sẽ đọc tệp này ở mọi bước. Xong gọi finish.{}",
        if exists { "Cập nhật (nếu yêu cầu sửa đổi làm thay đổi nguyên tắc)" } else { "Viết" },
        p.name,
        revision_note(p)
    );
    agent::run(store, Job { employee_id: ceo.id.clone(), project_id: p.id.clone(), phase: "plan", ticket_id: None, case_id: None, task }, cancel).await?;
    Ok(())
}

/// The BA finds the ambiguities that matter and asks the customer once (Spec Kit "clarify").
/// Returns the file holding the answers.
async fn clarify(store: &S, p: &Project, ba: &Employee, cancel: &CancellationToken) -> Result<String, String> {
    let file = if p.feedback.is_empty() { "docs/CLARIFICATIONS.md".to_string() } else { format!("docs/CLARIFICATIONS-sua-{}.md", p.feedback.len()) };
    if store.dir_of(p).join(&file).is_file() {
        return Ok(file);
    }
    let task = format!(
        "Trước khi viết đặc tả yêu cầu: đọc yêu cầu của khách, docs/PLAN.md, docs/DECISIONS.md, docs/CONSTITUTION.md và tìm các điểm MƠ HỒ ảnh hưởng lớn tới phạm vi, dữ liệu, trải nghiệm hoặc nghiệm thu (vd. có cần đăng nhập không, ai dùng, bao nhiêu dữ liệu, quy tắc tính toán, xử lý khi lỗi). Chọn TỐI ĐA 5 câu quan trọng nhất (bỏ qua chi tiết nhỏ có thể tự quyết hợp lý).\nNếu có câu cần hỏi: gọi ask_chairman ĐÚNG MỘT LẦN, đánh số từng câu, viết dễ hiểu cho người không rành kỹ thuật, mỗi câu kèm 2–3 lựa chọn gợi ý và \"Đề xuất: …\" để khách chỉ cần trả lời ngắn (vd. \"1A, 2 theo đề xuất\").\nGhi {file}: từng câu hỏi, câu trả lời của khách (hoặc \"theo đề xuất\" khi khách không trả lời / đồng ý đề xuất), và những giả định bạn tự quyết. Nếu không có điểm mơ hồ đáng kể thì không hỏi, chỉ ghi các giả định. Xong gọi finish.{}",
        revision_note(p)
    );
    agent::run(store, Job { employee_id: ba.id.clone(), project_id: p.id.clone(), phase: "clarify", ticket_id: None, case_id: None, task }, cancel).await?;
    Ok(file)
}

/// Cross-checks requirements ↔ plan ↔ architecture ↔ tickets before any code is written (Spec Kit "analyze").
async fn analyze(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let Some(ba) = first(store, &[Role::Ba, Role::Pm, Role::Architect]) else { return Ok(()) };
    let task = format!(
        "KIỂM TRA NHẤT QUÁN trước khi bắt đầu lập trình. Đọc docs/CONSTITUTION.md, docs/REQUIREMENTS.md, docs/PLAN.md, docs/ARCHITECTURE.md, docs/DATA.md, docs/DESIGN.md (nếu có) và danh sách ticket:\n{}\nViết docs/ANALYSIS.md:\n1. Bảng phủ: mỗi FR / user story → ticket nào làm (đánh dấu FR chưa có ticket).\n2. Ticket không gắn với yêu cầu nào (thừa).\n3. Mâu thuẫn giữa các tài liệu (thuật ngữ, dữ liệu, luồng, công nghệ).\n4. Vi phạm CONSTITUTION.\n5. Yêu cầu mơ hồ / không kiểm thử được.\nMỗi vấn đề ghi mức độ (Nghiêm trọng / Cao / Trung bình / Thấp). Với FR chưa có ticket hoặc vấn đề Nghiêm trọng / Cao: gọi create_ticket để bổ sung (đúng một module, ghi mã FR). Không sửa tài liệu của người khác. Xong gọi finish với tóm tắt số vấn đề theo mức độ.{}",
        ticket_list(store, &p.id),
        revision_note(p)
    );
    let r = agent::run(store, Job { employee_id: ba.id.clone(), project_id: p.id.clone(), phase: "analyze", ticket_id: None, case_id: None, task }, cancel).await?;
    store.post_message(Some(&p.id), &ba.id, "all", format!("Kiểm tra nhất quán trước khi code: {}{}", summary_of(&r.outcome), if r.created_tickets > 0 { format!(" (bổ sung {} ticket)", r.created_tickets) } else { String::new() }), "chat");
    Ok(())
}

/// The PM consults the customer (who is not technical) and agrees on the technology, once per project.
async fn consult_tech(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let doc = store.dir_of(p).join("docs").join("TECH-CHOICE.md");
    if !p.tech.trim().is_empty() || p.existing || !p.feedback.is_empty() || doc.is_file() {
        return Ok(());
    }
    let Some(pm) = first(store, &[Role::Pm, Role::Ceo]) else { return Ok(()) };
    if let Some(ceo) = first(store, &[Role::Ceo]).filter(|c| c.id != pm.id) {
        handoff(store, &p.id, &ceo.id, &pm.id, "Nhờ PM tư vấn và thống nhất công nghệ với khách hàng.");
    }
    let task = format!(
        "Khách hàng (Chủ tịch) đặt dự án:\n{}\n\nKhách hàng KHÔNG rành công nghệ. Với vai trò PM, hãy tư vấn và chốt công nghệ với khách:\n1. Tự phân tích yêu cầu và chọn 2–3 phương án công nghệ phù hợp (tra tài liệu mới nhất nếu có công cụ). Hướng chung của công ty: {}\n2. Gọi ask_chairman ĐÚNG MỘT LẦN. Viết cho người không biết kỹ thuật: mỗi phương án đặt tên dễ hiểu (vd. \"Phương án A — Ứng dụng web chạy trên trình duyệt\"), nói ý nghĩa thực tế với khách: dùng trên thiết bị nào, có cần tài khoản / máy chủ không, chi phí vận hành hằng tháng ước tính, thời gian làm, sau này mở rộng / sửa có dễ không, ưu / nhược. Ghi rõ phương án bạn ĐỀ XUẤT và vì sao. Tên công nghệ chỉ để trong ngoặc, không dùng thuật ngữ khó.\n3. Ghi docs/TECH-CHOICE.md: các phương án đã đưa, khách chọn gì (hoặc nếu khách không trả lời: dùng phương án đề xuất và ghi rõ là giả định), công nghệ cụ thể và phiên bản sẽ dùng cho đội kỹ thuật.\nXong gọi finish với tóm tắt lựa chọn.",
        p.request,
        crate::config::TECH_GUIDE
    );
    agent::run(store, Job { employee_id: pm.id.clone(), project_id: p.id.clone(), phase: "consult", ticket_id: None, case_id: None, task }, cancel).await?;
    store.post_message(Some(&p.id), &pm.id, "all", "Đã thống nhất công nghệ với khách hàng, xem docs/TECH-CHOICE.md.", "chat");
    Ok(())
}

/// For a customer's existing codebase: the Tech Lead studies it before the kickoff (once).
async fn onboard_existing(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let doc = store.dir_of(p).join("docs").join("EXISTING-CODE.md");
    if !p.existing || doc.is_file() {
        return Ok(());
    }
    let Some(lead) = first(store, &[Role::Architect, Role::Dev, Role::Ceo]) else { return Ok(()) };
    store.post_message(Some(&p.id), &lead.id, "all", format!("Đang đọc hiểu mã nguồn khách gửi ({}).", p.source), "chat");
    let task = format!(
        "Khách hàng gửi MÃ NGUỒN CÓ SẴN (nguồn: {}) để công ty sửa / nâng cấp theo yêu cầu:\n{}\n\nTrước khi họp khởi động, hãy đọc hiểu codebase (list_files, README, tệp cấu hình như package.json, điểm vào, các thư mục chính — đọc có chọn lọc). Nếu có thể, thử cài đặt / chạy test / build để biết hiện trạng. Viết docs/EXISTING-CODE.md: công nghệ và phiên bản; cách cài, chạy, test, build; cây thư mục và vai trò từng phần; các trang / module / luồng chính; mô hình dữ liệu và lưu trữ; chất lượng code, nợ kỹ thuật, lỗi / rủi ro thấy được; những chỗ cần đụng tới để đáp ứng yêu cầu của khách; đề xuất cách sửa an toàn. Không sửa code ở bước này. Xong gọi finish với tóm tắt hiện trạng.",
        p.source,
        p.request
    );
    agent::run(store, Job { employee_id: lead.id.clone(), project_id: p.id.clone(), phase: "onboarding", ticket_id: None, case_id: None, task }, cancel).await?;
    Ok(())
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

/// Kickoff: the CEO drafts a plan, the department heads discuss it over several rounds
/// (opinions, then rebuttals), and only then the CEO decides and finalises the plan.
async fn phase_plan(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let ceo = first(store, &[Role::Ceo]).ok_or("Công ty cần ít nhất 1 CEO đang làm việc")?;
    let all = members(store, p);
    // The project brief handed to the CEO (a hand-over, not something the chairman just typed: no reply expected).
    let brief = match p.feedback.last() {
        Some(fb) => format!("📋 Yêu cầu sửa đổi (vòng {}): {fb}", p.feedback.len()),
        None => format!("📋 Yêu cầu dự án: {}", p.request),
    };
    store.post_message(Some(&p.id), "chairman", &ceo.id, brief, "handoff");
    meeting(store, &p.id, all.clone(), "Họp khởi động", true);
    if let Err(e) = consult_tech(store, p, cancel).await {
        meeting(store, &p.id, all, "", false);
        return Err(e);
    }
    if let Err(e) = onboard_existing(store, p, cancel).await {
        meeting(store, &p.id, all, "", false);
        return Err(e);
    }
    let r = match kickoff(store, p, &ceo, &all, cancel).await {
        Ok(r) => r,
        Err(e) => {
            meeting(store, &p.id, all, "", false);
            return Err(e);
        }
    };
    if is_standard(p) {
        // Medium project: the CEO's plan is the plan (no debate, vote, constitution or minutes).
        meeting(store, &p.id, all, "", false);
        let next = first(store, &[Role::Ba, Role::Pm]).unwrap_or(ceo.clone());
        if next.id != ceo.id {
            handoff(store, &p.id, &ceo.id, &next.id, format!("Kế hoạch đã chốt (docs/PLAN.md). {}", summary_of(&r.outcome)));
        }
        return Ok(());
    }
    if let Err(e) = write_constitution(store, p, &ceo, cancel).await {
        meeting(store, &p.id, all, "", false);
        return Err(e);
    }
    meeting(store, &p.id, all.clone(), "Họp khởi động", true);
    let minutes_result = minutes(store, p, "Họp khởi động", "docs/PLAN.md, docs/DECISIONS.md (nếu có) và các ý kiến trong docs/meetings/", cancel).await;
    meeting(store, &p.id, all, "", false);
    minutes_result?;
    let next = first(store, &[Role::Ba, Role::Pm]).unwrap_or(ceo.clone());
    if next.id != ceo.id {
        handoff(store, &p.id, &ceo.id, &next.id, format!("Kế hoạch đã chốt (docs/PLAN.md). {}", summary_of(&r.outcome)));
    }
    Ok(())
}

/// One person per department speaks at the kickoff discussion.
fn discussion_panel(store: &S) -> Vec<Employee> {
    const ROLES: [Role; 14] = [
        Role::Ba,
        Role::Pm,
        Role::Architect,
        Role::DataEngineer,
        Role::Security,
        Role::Designer,
        Role::Dev,
        Role::Reviewer,
        Role::Tester,
        Role::Devops,
        Role::Marketing,
        Role::Sales,
        Role::Legal,
        Role::Accountant,
    ];
    ROLES.iter().filter_map(|r| team_staff(store, *r).into_iter().next()).collect()
}

async fn kickoff(store: &S, p: &Project, ceo: &Employee, all: &[String], cancel: &CancellationToken) -> Result<agent::JobResult, String> {
    // The debate and vote are for big projects only.
    let rounds = if is_standard(p) { 0 } else { store.with(|s| s.settings.meeting_rounds).min(5) };
    let panel = if rounds == 0 { vec![] } else { discussion_panel(store) };
    let dir = if p.feedback.is_empty() { "docs/meetings/kickoff".to_string() } else { format!("docs/meetings/kickoff-sua-{}", p.feedback.len()) };
    // Resumable: whatever is already written to disk (opinions, ballot, votes) is not redone after a restart.
    let pdir = store.dir_of(p);
    let has = |rel: &str| std::fs::metadata(pdir.join(rel)).map(|m| m.len() > 0).unwrap_or(false);
    let started = !panel.is_empty()
        && std::fs::read_dir(pdir.join(&dir)).map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with("v1-"))).unwrap_or(false);

    // 1. The CEO drafts.
    let tech_step = if !p.tech.trim().is_empty() {
        format!("CÔNG NGHỆ: Chủ tịch đã chỉ định \"{}\". Bắt buộc dùng, ghi rõ vào PLAN.md.", p.tech.trim())
    } else if p.existing {
        "CÔNG NGHỆ: giữ công nghệ của mã nguồn có sẵn (docs/EXISTING-CODE.md); chỉ thêm thư viện mới khi thật cần và nêu lý do. Đọc docs/EXISTING-CODE.md trước khi viết kế hoạch.".to_string()
    } else if p.feedback.is_empty() {
        format!(
            "CÔNG NGHỆ: PM đã tư vấn và thống nhất với khách hàng trong docs/TECH-CHOICE.md — đọc và dùng đúng lựa chọn đó, ghi vào PLAN.md. Nếu tệp không có, chọn theo hướng công ty ({}) và ghi rõ là giả định.",
            crate::config::TECH_GUIDE
        )
    } else {
        "CÔNG NGHỆ: giữ công nghệ đã chốt trong docs/PLAN.md, chỉ đổi khi Chủ tịch yêu cầu.".to_string()
    };
    let draft_note = if panel.is_empty() {
        ""
    } else {
        " Đây là BẢN NHÁP để đưa ra cuộc họp: các trưởng bộ phận sẽ góp ý và phản biện trước khi bạn chốt, nên hãy nêu rõ các phương án bạn còn phân vân."
    };
    let task = format!(
        "Chủ tịch giao dự án \"{}\":\n{}{}\n\nViệc của bạn: phân tích yêu cầu và viết kế hoạch ngắn gọn vào docs/PLAN.md (mục tiêu, phạm vi, công nghệ, danh sách trang / module chính, ai trong công ty làm gì).{draft_note}\n\n{tech_step}\n\nNgoài việc chọn công nghệ, chỉ hỏi Chủ tịch (ask_chairman) khi thật sự không thể tự quyết. Xong gọi finish.",
        p.name,
        p.request,
        revision_note(p)
    );
    if started {
        store.post_message(Some(&p.id), &ceo.id, "all", "Tiếp tục cuộc họp đang dở, ai đã góp ý / bỏ phiếu thì giữ nguyên.", "chat");
    } else {
        let draft = agent::run(store, Job { employee_id: ceo.id.clone(), project_id: p.id.clone(), phase: "plan", ticket_id: None, case_id: None, task }, cancel).await?;
        if panel.is_empty() {
            return Ok(draft);
        }
        store.post_message(Some(&p.id), &ceo.id, "all", format!("Bản nháp kế hoạch ở docs/PLAN.md, mời các anh chị góp ý thẳng thắn. {}", summary_of(&draft.outcome)), "chat");
    }

    // 2. Discussion rounds: everyone on the panel speaks in parallel.
    for round in 1..=rounds {
        let ids: Vec<String> = std::iter::once(ceo.id.clone()).chain(panel.iter().map(|e| e.id.clone())).collect();
        let topic = if round == 1 { "Họp khởi động · góp ý".to_string() } else { format!("Họp khởi động · phản biện vòng {round}") };
        meeting(store, &p.id, ids, &topic, true);
        let mut handles = vec![];
        let mut spoke = 0;
        for emp in &panel {
            let slug = format!("{:?}", emp.role).to_lowercase();
            let file = format!("{dir}/v{round}-{slug}.md");
            if has(&file) {
                spoke += 1;
                continue;
            }
            let role = crate::config::role_label(emp.role);
            let task = if round == 1 {
                format!(
                    "Cuộc họp khởi động dự án \"{}\". Yêu cầu của Chủ tịch:\n{}\n\nCEO đã đưa bản nháp kế hoạch ở docs/PLAN.md. Với góc nhìn chuyên môn của {role}, đọc kế hoạch rồi viết ý kiến vào {file}:\n1. Điểm bạn đồng ý.\n2. Rủi ro / vấn đề bạn thấy (cụ thể, có lý do).\n3. Đề xuất để tối ưu: đơn giản hơn, nhanh hơn, rẻ hơn hoặc chất lượng hơn.\n4. Phương án thay thế nếu bạn không đồng ý với hướng đi.\n5. Phần việc của bạn: cần gì, ước lượng công sức.\nCông nghệ do Chủ tịch chọn trong PLAN.md là cố định (chỉ góp ý cách dùng). Góp ý cả cách chia trang / module cho dễ nâng cấp. Thẳng thắn, có lập luận — không khen xã giao, không chép lại kế hoạch. Ngắn gọn, khoảng một trang, gạch đầu dòng, viết trọn câu — ghi một lần là xong, không cần đếm chữ hay sửa đi sửa lại. Chỉ ghi đúng tệp này, không sửa tệp khác. Xong gọi finish với tóm tắt 1–2 câu ý kiến chính của bạn.",
                    p.name, p.request
                )
            } else {
                format!(
                    "Họp khởi động dự án \"{}\", vòng phản biện {round}. Đọc docs/PLAN.md và TẤT CẢ ý kiến vòng trước trong {dir}/ (các tệp v{}-*.md). Viết vào {file}:\n1. Ý kiến của ai bạn ủng hộ và vì sao.\n2. Ý kiến của ai bạn phản đối, kèm lập luận và phương án tốt hơn.\n3. Những điểm còn mâu thuẫn cần CEO quyết.\n4. Đề xuất cuối cùng của bạn (đã điều chỉnh sau khi nghe mọi người).\nĐi thẳng vào vấn đề, ngắn gọn khoảng một trang, viết trọn câu — ghi một lần là xong, không cần đếm chữ hay sửa đi sửa lại. Chỉ ghi đúng tệp này. Xong gọi finish với tóm tắt 1–2 câu lập trường của bạn.",
                    p.name,
                    round - 1
                )
            };
            let store = store.clone();
            let pid = p.id.clone();
            let cancel = cancel.clone();
            let emp = emp.clone();
            let out = pdir.join(&file);
            handles.push(tokio::spawn(async move {
                let r = agent::run(&store, Job { employee_id: emp.id.clone(), project_id: pid.clone(), phase: "meeting_opinion", ticket_id: None, case_id: None, task }, &cancel).await;
                match r {
                    Ok(r) => {
                        store.post_message(Some(&pid), &emp.id, "all", summary_of(&r.outcome), "chat");
                        true
                    }
                    Err(e) if e != ABORTED && std::fs::metadata(&out).map(|m| m.len() > 0).unwrap_or(false) => {
                        // Ran out of steps while polishing, but the opinion is on disk: it counts.
                        store.post_message(Some(&pid), &emp.id, "all", "Đã ghi ý kiến.", "chat");
                        true
                    }
                    Err(e) => {
                        if e != ABORTED {
                            store.post_message(Some(&pid), &emp.id, "all", format!("(Không góp ý được: {})", e.chars().take(120).collect::<String>()), "chat");
                        }
                        false
                    }
                }
            }));
        }
        for h in handles {
            if matches!(h.await, Ok(true)) {
                spoke += 1;
            }
        }
        if cancel.is_cancelled() {
            return Err(ABORTED.into());
        }
        if spoke == 0 {
            break; // nobody could speak (e.g. the provider is down): don't waste more rounds
        }
    }

    let voters: Vec<Employee> = std::iter::once(ceo.clone()).chain(panel.iter().cloned()).collect();
    let voter_ids: Vec<String> = voters.iter().map(|e| e.id.clone()).collect();

    // 3. The CEO (as chair) turns the open disagreements into a ballot.
    meeting(store, &p.id, voter_ids.clone(), "Họp khởi động · lập phiếu bầu", true);
    let ballot_file = format!("{dir}/BALLOT.md");
    let task = format!(
        "Bạn chủ trì cuộc họp khởi động dự án \"{}\". Đọc docs/PLAN.md và toàn bộ ý kiến, phản biện trong {dir}/. Tổng hợp các vấn đề còn tranh cãi hoặc có nhiều cách làm (tối đa 6 vấn đề quan trọng nhất) thành phiếu bầu, ghi vào {ballot_file} ĐÚNG định dạng sau:\n\n## V1: <vấn đề cần quyết>\n- A: <phương án> (đề xuất bởi <vai trò>)\n- B: <phương án> (đề xuất bởi <vai trò>)\n\n## V2: ...\n\nKhông đưa lựa chọn công nghệ của Chủ tịch ra bỏ phiếu. Mỗi vấn đề 2–4 phương án, trình bày công bằng, ghi kèm ưu/nhược ngắn gọn của từng phương án ngay trên dòng đó. Không tự quyết ở bước này. Nếu mọi người đã đồng thuận hết thì ghi tệp chỉ với dòng \"Không có vấn đề cần bỏ phiếu\". Xong gọi finish.",
        p.name
    );
    if !has(&ballot_file) {
        agent::run(store, Job { employee_id: ceo.id.clone(), project_id: p.id.clone(), phase: "meeting_ballot", ticket_id: None, case_id: None, task }, cancel).await?;
    }
    let issues = ballot_issues(&std::fs::read_to_string(pdir.join(&ballot_file)).unwrap_or_default());

    // 4. Everyone votes in parallel; 5. the votes are counted in code.
    let mut result_text = String::new();
    if !issues.is_empty() {
        meeting(store, &p.id, voter_ids.clone(), "Họp khởi động · bỏ phiếu", true);
        store.post_message(Some(&p.id), &ceo.id, "all", format!("Có {} vấn đề cần bỏ phiếu, xem {ballot_file}. Mời mọi người bỏ phiếu.", issues.len()), "chat");
        let mut handles = vec![];
        for emp in &voters {
            let slug = format!("{:?}", emp.role).to_lowercase();
            let file = format!("{dir}/votes/{slug}.md");
            let voted = has(&file);
            let role = crate::config::role_label(emp.role);
            let ids = issues.iter().map(|(id, _, _)| id.as_str()).collect::<Vec<_>>().join(", ");
            let task = format!(
                "Bỏ phiếu ở cuộc họp khởi động dự án \"{}\". Đọc phiếu bầu {ballot_file}, docs/PLAN.md và các ý kiến trong {dir}/. Với tư cách {role}, chọn phương án bạn cho là TỐT NHẤT cho dự án (không phải phương án có lợi cho riêng bộ phận bạn) ở từng vấn đề: {ids}. Ghi vào {file}, mỗi vấn đề đúng một dòng theo định dạng:\nV1: B — <lý do một câu>\nChỉ ghi đúng tệp này. Xong gọi finish với tóm tắt lựa chọn của bạn.",
                p.name
            );
            let store = store.clone();
            let pid = p.id.clone();
            let cancel = cancel.clone();
            let emp = emp.clone();
            handles.push(tokio::spawn(async move {
                if voted {
                    return (emp, file);
                }
                let r = agent::run(&store, Job { employee_id: emp.id.clone(), project_id: pid.clone(), phase: "meeting_vote", ticket_id: None, case_id: None, task }, &cancel).await;
                if let Ok(r) = &r {
                    store.post_message(Some(&pid), &emp.id, "all", format!("Phiếu của tôi: {}", summary_of(&r.outcome)), "chat");
                }
                (emp, file)
            }));
        }
        let mut ballots = vec![];
        for h in handles {
            if let Ok((emp, file)) = h.await {
                let text = std::fs::read_to_string(pdir.join(&file)).unwrap_or_default();
                ballots.push((emp, parse_votes(&text)));
            }
        }
        if cancel.is_cancelled() {
            return Err(ABORTED.into());
        }
        result_text = tally(&issues, &ballots, &ceo.id);
        let _ = std::fs::write(pdir.join(format!("{dir}/RESULTS.md")), &result_text);
        let short: Vec<String> = result_text.lines().filter(|l| l.starts_with("**V")).map(|l| l.replace("**", "")).collect();
        store.post_message(Some(&p.id), &ceo.id, "all", format!("Kết quả bỏ phiếu: {}", short.join(" · ")), "chat");
    }

    // 6. The CEO finalises the plan, bound by the vote.
    meeting(store, &p.id, all.to_vec(), "Họp khởi động · CEO chốt phương án", true);
    let vote_rule = if result_text.is_empty() {
        "Không có vấn đề phải bỏ phiếu: chốt theo đồng thuận của cuộc họp.".to_string()
    } else {
        format!("Kết quả bỏ phiếu (đã đếm, BẮT BUỘC tuân theo — phương án thắng phiếu là quyết định của công ty, bạn không được đổi):\n{result_text}")
    };
    let task = format!(
        "Cuộc họp khởi động dự án \"{}\" đã thảo luận và bỏ phiếu xong. Đọc docs/PLAN.md (bản nháp), các ý kiến trong {dir}/ và phiếu bầu {ballot_file}.\n{vote_rule}\n\nViệc của bạn:\n- Viết docs/DECISIONS.md: với mỗi vấn đề, các phương án, số phiếu, phương án được chọn và lý do (tóm lược lập luận của phe thắng và phe thua); các đề xuất khác được chấp nhận / bị bác.\n- Cập nhật docs/PLAN.md thành bản kế hoạch CHỐT cuối cùng, áp dụng đúng các phương án thắng phiếu.\nXong gọi finish với tóm tắt các quyết định chính.{}",
        p.name,
        revision_note(p)
    );
    let r = agent::run(store, Job { employee_id: ceo.id.clone(), project_id: p.id.clone(), phase: "plan_decide", ticket_id: None, case_id: None, task }, cancel).await?;
    store.post_message(Some(&p.id), &ceo.id, "all", format!("Chốt phương án: {}", summary_of(&r.outcome)), "chat");
    Ok(r)
}

/// Issues on the ballot: `## V1: question` followed by `- A: option` lines → (id, question, [(letter, option)]).
fn ballot_issues(text: &str) -> Vec<(String, String, Vec<(char, String)>)> {
    let mut out: Vec<(String, String, Vec<(char, String)>)> = vec![];
    for line in text.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("## ") {
            if let Some((id, q)) = rest.split_once(':') {
                let id = id.trim().to_uppercase();
                if id.starts_with('V') && id[1..].chars().all(|c| c.is_ascii_digit()) && id.len() > 1 {
                    out.push((id, q.trim().to_string(), vec![]));
                }
            }
        } else if let Some(rest) = l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")) {
            let mut cs = rest.chars();
            if let (Some(c), Some(':')) = (cs.next(), cs.next()) {
                if c.is_ascii_alphabetic() {
                    if let Some(last) = out.last_mut() {
                        last.2.push((c.to_ascii_uppercase(), cs.as_str().trim().to_string()));
                    }
                }
            }
        }
    }
    out.retain(|(_, _, opts)| opts.len() >= 2);
    out
}

/// A ballot file: lines like `V1: B — reason` → [(issue id, letter)].
fn parse_votes(text: &str) -> Vec<(String, char)> {
    let mut out = vec![];
    for line in text.lines() {
        let l = line.trim().trim_start_matches(['-', '*', '#', ' ']).replace("**", "");
        let Some((id, rest)) = l.split_once(':') else { continue };
        let id = id.trim().to_uppercase();
        if !(id.len() > 1 && id.starts_with('V') && id[1..].chars().all(|c| c.is_ascii_digit())) {
            continue;
        }
        if let Some(c) = rest.trim().chars().next().filter(|c| c.is_ascii_alphabetic()) {
            if !out.iter().any(|(i, _): &(String, char)| *i == id) {
                out.push((id, c.to_ascii_uppercase()));
            }
        }
    }
    out
}

/// Counts the votes. The option with the most votes wins; a tie goes to the option the CEO voted for,
/// otherwise to the earliest option.
fn tally(issues: &[(String, String, Vec<(char, String)>)], ballots: &[(Employee, Vec<(String, char)>)], ceo_id: &str) -> String {
    let mut md = String::from("# Kết quả bỏ phiếu\n\n");
    for (id, question, opts) in issues {
        let mut counts: Vec<(char, Vec<String>)> = opts.iter().map(|(c, _)| (*c, vec![])).collect();
        let mut ceo_pick = None;
        for (emp, votes) in ballots {
            if let Some((_, c)) = votes.iter().find(|(i, _)| i == id) {
                if let Some(slot) = counts.iter_mut().find(|(o, _)| o == c) {
                    slot.1.push(format!("{} ({})", emp.name, crate::config::role_label(emp.role)));
                    if emp.id == ceo_id {
                        ceo_pick = Some(*c);
                    }
                }
            }
        }
        let max = counts.iter().map(|(_, v)| v.len()).max().unwrap_or(0);
        let tied: Vec<char> = counts.iter().filter(|(_, v)| v.len() == max).map(|(c, _)| *c).collect();
        let winner = if tied.len() > 1 { ceo_pick.filter(|c| tied.contains(c)).unwrap_or(tied[0]) } else { tied[0] };
        let win_text = opts.iter().find(|(c, _)| *c == winner).map(|(_, t)| t.as_str()).unwrap_or("");
        let note = if tied.len() > 1 && max > 0 { " (hòa phiếu, phiếu CEO quyết)" } else { "" };
        md.push_str(&format!("## {id}: {question}\n\n**{id} → {winner}: {max} phiếu{note}**\n\n"));
        for (c, voters) in &counts {
            let t = opts.iter().find(|(o, _)| o == c).map(|(_, t)| t.as_str()).unwrap_or("");
            md.push_str(&format!("- {c} ({} phiếu): {t}\n  - {}\n", voters.len(), if voters.is_empty() { "—".into() } else { voters.join(", ") }));
        }
        md.push_str(&format!("\nPhương án chọn: **{winner}** — {win_text}\n\n"));
    }
    md
}

async fn phase_requirements(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let ba = first(store, &[Role::Ba, Role::Pm, Role::Ceo]).ok_or("Không có ai viết yêu cầu (BA/PM/CEO)")?;
    let clar = clarify(store, p, &ba, cancel).await?;
    let task = format!(
        "Đọc docs/PLAN.md, docs/DECISIONS.md, docs/CONSTITUTION.md và {clar} (câu trả lời làm rõ của khách — bắt buộc tuân theo). Viết docs/REQUIREMENTS.md theo MẪU ĐẶC TẢ CHUẨN:\n1. User story theo độ ưu tiên (P1 = bắt buộc cho bản đầu, P2, P3); mỗi story kiểm thử độc lập được, ghi \"Vì sao ưu tiên này\".\n2. Kịch bản nghiệm thu dạng Cho trước / Khi / Thì (Given / When / Then) cho từng story.\n3. Ca biên (edge cases).\n4. Yêu cầu chức năng đánh mã FR-001, FR-002… (mỗi yêu cầu một câu \"Hệ thống PHẢI …\", kiểm thử được, không mô tả cách làm kỹ thuật).\n5. Thực thể dữ liệu chính (nếu có).\n6. Tiêu chí thành công ĐO ĐƯỢC SC-001… (vd. \"người dùng ghi một khoản chi trong dưới 10 giây\"), không phụ thuộc công nghệ.\n7. Giả định và ngoài phạm vi.\nCHỈ đặc tả đúng những gì khách yêu cầu: KHÔNG tự thêm máy chủ / backend, tài khoản, cơ sở dữ liệu, thống kê, chia sẻ mạng xã hội hay tính năng phụ khi khách không yêu cầu — ghi những ý đó vào mục Ngoài phạm vi.{{size_note}}
Chia theo trang / module. Sau đó tạo ticket bằng create_ticket: {{ticket_range}} ticket nhỏ, MỖI TICKET THUỘC ĐÚNG MỘT TRANG HOẶC MỘT MODULE (ghi tên ở đầu tiêu đề, vd. \"[module giao-dich] Thêm / sửa / xóa giao dịch\"), ghi rõ các mã FR mà ticket đáp ứng trong mô tả, làm được độc lập, theo thứ tự nên làm, tiêu chí nghiệm thu kiểm tra được. Mọi FR phải thuộc ít nhất một ticket. Xong gọi finish.{}",
        revision_note(p)
    );
    let task = task
        .replace("{size_note}", if is_standard(p) { " Dự án cỡ vừa: tài liệu NGẮN GỌN (1–2 trang), bỏ qua mục không cần." } else { "" })
        .replace("{ticket_range}", if is_standard(p) { "2–6" } else { "2–8" });
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
    let preview = crate::config::preview_url(&p.id);
    let dir = store.dir_of(p);
    let run = |task: String| agent::run(store, Job { employee_id: designer.id.clone(), project_id: p.id.clone(), phase: "design", ticket_id: None, case_id: None, task }, cancel);
    // First design of a project the chairman approves: offer several directions to pick from.
    if design_option_count(store, p) > 1 && !dir.join("design").join("mockup.html").is_file() && store.with(|s| s.settings.approve_design) {
        // One job per option: each gets the full step budget, options already made (e.g. before a
        // restart) are kept, and a late failure still leaves the ones that were finished.
        for (i, id) in DESIGN_OPTION_IDS.iter().enumerate() {
            // The chairman may change the number from the chat while options are being made.
            let n = store.project(&p.id).map(|q| design_option_count(store, &q)).unwrap_or(1);
            if i >= n {
                break;
            }
            if dir.join("design/options").join(id).join("mockup.html").is_file() {
                continue;
            }
            let made = design_options(&dir);
            if let Err(e) = run(design_option_task(&preview, id, n, &made, &revision_note(p))).await {
                if e == "aborted" || cancel.is_cancelled() {
                    return Err(e);
                }
                if dir.join("design/options").join(id).join("mockup.html").is_file() {
                    continue; // The option is there; only the polishing ran out of steps.
                }
                if design_options(&dir).len() >= 2 {
                    store.post_message(Some(&p.id), &designer.id, "chairman", format!("Phương án {} chưa làm xong ({}), em gửi anh/chị các phương án đã hoàn thành.", id.to_uppercase(), short_text(&e, 120)), "chat");
                    break;
                }
                return Err(e);
            }
        }
        let opts = design_options(&dir);
        if !opts.is_empty() {
            let single = opts.len() == 1;
            let (pick, feedback) = if single { (opts[0].clone(), None) } else { choose_design(store, p, &designer, opts, cancel).await? };
            run(design_final_task(&preview, &pick, feedback.as_deref(), &revision_note(p))).await?;
            // The chairman already chose; confirm again only when there were changes to make (or nothing to choose from).
            return if single || feedback.is_some() { approve_design(store, p, &designer, cancel).await } else { Ok(()) };
        }
        // No options were produced: fall back to a single design.
    }
    run(format!(
        "Đọc docs/PLAN.md, docs/DECISIONS.md và docs/REQUIREMENTS.md. Thiết kế giao diện ĐẸP, hiện đại, chuyên nghiệp theo kỹ năng Designer của bạn:\n{DESIGN_DELIVERABLES}\nNếu có công cụ trình duyệt: mở {preview}design/mockup.html, chụp màn hình ở 1280px và 375px, tự soi và sửa tới khi đẹp, cân đối, không lỗi. Xong gọi finish.{}",
        revision_note(p)
    ))
    .await?;
    approve_design(store, p, &designer, cancel).await
}

const DESIGN_DELIVERABLES: &str = "- docs/DESIGN.md: phong cách, hệ màu (mã hex, có tương phản đạt WCAG AA), font hỗ trợ tiếng Việt, thang cỡ chữ, thang khoảng cách, bo góc, bóng đổ, bố cục từng trang (desktop và điện thoại), từng thành phần với đủ trạng thái (hover, focus, disabled, loading, rỗng, lỗi).\n- design/tokens.css: toàn bộ biến CSS để Dev dùng lại (nguồn duy nhất).\n- design/mockup.html: bản mẫu tĩnh tự chứa (import ./tokens.css) thể hiện các trang chính và trạng thái quan trọng.";

/// Design directions offered to the chairman (folders design/options/<id>/).
const DESIGN_OPTION_IDS: [&str; 8] = ["a", "b", "c", "d", "e", "f", "g", "h"];

/// Design directions offered for a project: its own number (set from chat), else the company setting.
/// Design directions offered for a project: its own number (set from chat), else the company setting.
pub fn design_option_count(store: &S, p: &Project) -> usize {
    let n = if p.design_options > 0 { p.design_options } else { store.with(|s| s.settings.design_options) };
    (n as usize).clamp(1, DESIGN_OPTION_IDS.len())
}

pub fn set_design_options(store: &S, project_id: Option<&str>, n: u8) {
    let n = n.clamp(1, DESIGN_OPTION_IDS.len() as u8);
    match project_id {
        Some(id) => {
            store.update_project(id, |p| p.design_options = n);
        }
        None => update_settings(store, |s| s.design_options = n),
    }
}

/// The team decides alone (company-wide or for this project): no question waits for the chairman.
pub fn autopilot(store: &S, project_id: Option<&str>) -> bool {
    store.with(|s| s.settings.autopilot) || project_id.and_then(|id| store.project(id)).is_some_and(|p| p.autopilot)
}

pub fn set_autopilot(store: &S, project_id: Option<&str>, on: bool) {
    match project_id {
        Some(id) => {
            store.update_project(id, |p| p.autopilot = on);
        }
        None => update_settings(store, |s| s.autopilot = on),
    }
}

fn update_settings(store: &S, f: impl FnOnce(&mut Settings)) {
    let settings = store.with(|s| {
        f(&mut s.settings);
        s.settings.clone()
    });
    store.emit(crate::types::ServerEvent::Settings { settings });
}

/// Questions already waiting for the chairman in that scope are left to the team (as if skipped).
pub fn release_pending(store: &S, project_id: Option<&str>) -> usize {
    let ids: Vec<String> = store.with(|s| {
        s.approvals
            .iter()
            .filter(|a| a.status == ApprovalStatus::Pending && a.kind == ApprovalKind::Question && (project_id.is_none() || a.project_id.as_deref() == project_id))
            .map(|a| a.id.clone())
            .collect()
    });
    ids.iter().filter(|id| store.resolve_approval(id, false, None).is_some()).count()
}

/// A paused draft project for work about another product (from chat): its own folder, nothing runs
/// until the chairman presses Continue.
pub fn draft_project(store: &S, name: &str, request: &str) -> Project {
    let id = crate::store::new_id();
    let base = crate::api::slugify(name);
    let p = Project {
        id: id.clone(),
        name: name.chars().take(80).collect(),
        slug: format!("{}-{}", if base.is_empty() { "du-an" } else { &base }, &id[id.len() - 4..]),
        request: request.chars().take(2000).collect(),
        feedback: vec![],
        phase: Phase::Plan,
        status: ProjectStatus::Paused,
        budget_usd: store.with(|s| s.settings.default_budget_usd),
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
        gate_rounds: 0,
        change_scope: String::new(),
        mode: String::new(),
        meetings: 0,
        tech: String::new(),
        versions: vec![],
        existing: false,
        source: String::new(),
        dir: String::new(),
        created_at: now_ms(),
        updated_at: now_ms(),
    };
    let _ = std::fs::create_dir_all(store.dir_of(&p));
    store.insert_project(p.clone());
    p
}

/// The chairman asks for changes (project finished, paused or stopped): a new revision round from the kickoff.
pub fn request_changes(store: &S, id: &str, feedback: &str) -> Result<(), String> {
    store.project(id).ok_or("Dự án không tồn tại")?;
    if store.cancels.lock().unwrap().contains_key(id) {
        return Err("dự án đang chạy (tạm dừng dự án rồi nhắn lại để mở vòng sửa đổi); yêu cầu đã được gửi tới người đang làm".into());
    }
    // A live product keeps its customers; operations pause while the team builds the new version.
    store.stop_ops(id);
    store.update_project(id, |p| {
        p.feedback.push(feedback.trim().to_string());
        p.phase = Phase::Plan;
        p.change_scope.clear();
        p.gate_rounds = 0;
        p.release_rounds = 0;
        p.design_reviews = 0;
        p.qa_rounds = 0;
        p.security_reviews = 0;
    });
    start(store, id);
    Ok(())
}

fn design_option_task(preview: &str, id: &str, n: usize, made: &[ApprovalOption], revision: &str) -> String {
    let x = id.to_uppercase();
    let others = if made.is_empty() {
        String::new()
    } else {
        let list: Vec<String> = made.iter().map(|o| format!("{}. \"{}\"", o.id.to_uppercase(), o.title)).collect();
        format!(" Các phương án đã có (đọc README.md của chúng; phương án {x} phải KHÁC HẲN về phong cách, bảng màu, font, bố cục, cảm giác — không chỉ đổi màu): {}.", list.join(", "))
    };
    let lead = if id == "a" { " Đây là phương án bạn đề xuất nhất." } else { "" };
    format!(
        "Đọc docs/PLAN.md, docs/DECISIONS.md và docs/REQUIREMENTS.md. Chủ tịch sẽ XEM ẢNH và CHỌN 1 trong {n} hướng thiết kế. Lượt này bạn CHỈ làm PHƯƠNG ÁN {x}, đẹp, hiện đại, chuyên nghiệp theo kỹ năng Designer của bạn.{lead}{others}
Tạo trong design/options/{id}/:
- README.md: dòng đầu \"# <Tên phương án ngắn gọn>\", sau đó 2–4 câu tiếng Việt: ý tưởng, cảm giác, hợp với ai, màu chính (hex), font.
- tokens.css: biến CSS của phương án.
- mockup.html: bản mẫu tĩnh tự chứa (import ./tokens.css) của MÀN HÌNH QUAN TRỌNG NHẤT với nội dung thật, hoàn chỉnh, đẹp ở cả máy tính và điện thoại. Chưa cần làm mọi trang.
- ẢNH CHỤP (bắt buộc nếu có công cụ trình duyệt): mở {preview}design/options/{id}/mockup.html, browser_resize 1280x800 rồi browser_take_screenshot với filename \"design/options/{id}/desktop.png\" và fullPage true; browser_resize 375x812 rồi chụp filename \"design/options/{id}/mobile.png\" (fullPage true). Xem ảnh, sửa chỗ chưa đẹp rồi chụp lại — tối đa 2 vòng sửa, đủ đẹp thì dừng.
Không sửa phương án khác, chưa viết docs/DESIGN.md hay design/mockup.html. Xong gọi finish với tóm tắt phương án {x}.{revision}"
    )
}

fn design_final_task(preview: &str, pick: &ApprovalOption, feedback: Option<&str>, revision: &str) -> String {
    let fb = feedback.map(|f| format!("\nGóp ý thêm của Chủ tịch (phải làm theo): {f}")).unwrap_or_default();
    format!(
        "Chủ tịch đã CHỌN phương án {} \"{}\" (thư mục design/options/{}/).{fb}\nHoàn thiện thiết kế chính thức từ phương án này, giữ đúng phong cách đã chọn:\n{DESIGN_DELIVERABLES}\n(design/tokens.css phát triển từ design/options/{}/tokens.css.) Không xoá design/options/.\nNếu có công cụ trình duyệt: mở {preview}design/mockup.html, chụp màn hình ở 1280px và 375px, tự soi và sửa tới khi đẹp, cân đối, không lỗi. Xong gọi finish.{revision}",
        pick.id.to_uppercase(),
        pick.title,
        pick.id,
        pick.id
    )
}

/// Options the designer produced: a folder with a mockup, a title from its README and any screenshots.
fn design_options(dir: &Path) -> Vec<ApprovalOption> {
    design_options_at(dir, "design/options")
}

/// A headless Chrome / Edge on this machine, for screenshots the designer didn't take.
fn headless_browser() -> Option<std::path::PathBuf> {
    let mut cands: Vec<std::path::PathBuf> = vec![];
    if cfg!(windows) {
        for base in [std::env::var("ProgramFiles(x86)").ok(), std::env::var("ProgramFiles").ok(), std::env::var("LOCALAPPDATA").ok()].into_iter().flatten() {
            cands.push(Path::new(&base).join("Microsoft/Edge/Application/msedge.exe"));
            cands.push(Path::new(&base).join("Google/Chrome/Application/chrome.exe"));
        }
    } else {
        for b in ["/usr/bin/chromium", "/usr/bin/chromium-browser", "/usr/bin/google-chrome", "/usr/bin/microsoft-edge", "/snap/bin/chromium"] {
            cands.push(b.into());
        }
        // The browser Playwright downloaded for the employees' MCP.
        if let Ok(home) = std::env::var("HOME") {
            if let Ok(rd) = std::fs::read_dir(Path::new(&home).join(".cache/ms-playwright")) {
                for d in rd.flatten() {
                    cands.push(d.path().join("chrome-linux/chrome"));
                    cands.push(d.path().join("chrome-linux64/chrome"));
                }
            }
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// Screenshots (1280px and 375px) for design options that have none, so the chairman always sees pictures.
pub(crate) async fn ensure_shots(p: &Project, dir: &Path, opts: &mut [ApprovalOption]) {
    let Some(bin) = headless_browser() else { return };
    for o in opts.iter_mut().filter(|o| o.images.is_empty()) {
        let folder = o.link.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        let url = format!("{}{}", crate::config::preview_url(&p.id), o.link);
        for (file, size) in [("desktop.png", "1280,900"), ("mobile.png", "390,844")] {
            let out = dir.join(&folder).join(file);
            let profile = std::env::temp_dir().join(format!("aico-shot-{}", crate::store::new_id()));
            let run = tokio::process::Command::new(&bin)
                .args(["--headless=new", "--disable-gpu", "--hide-scrollbars", "--no-first-run", "--no-default-browser-check"])
                .arg(format!("--user-data-dir={}", profile.display()))
                .arg(format!("--window-size={size}"))
                .arg(format!("--screenshot={}", out.display()))
                .arg(&url)
                .kill_on_drop(true)
                .output();
            let _ = tokio::time::timeout(Duration::from_secs(40), run).await;
            let _ = std::fs::remove_dir_all(&profile);
            if out.is_file() {
                o.images.push(format!("{folder}/{file}"));
            }
        }
    }
}

/// "A · Gambit" / "Phương án A: Gambit" → "Gambit" (the card already shows the letter).
fn strip_option_letter(title: &str, id: &str) -> String {
    let up = id.to_uppercase();
    let mut t = title.trim();
    for prefix in ["Phương án ", "Mẫu ", "Option ", ""] {
        if let Some(rest) = t.strip_prefix(&format!("{prefix}{up}")).or_else(|| t.strip_prefix(&format!("{prefix}{id}"))) {
            let rest = rest.trim_start();
            // Only when the letter is followed by a separator, so "Aurora" stays "Aurora".
            if let Some(r) = rest.strip_prefix(['·', '.', ':', '-', '–', '—', ')']) {
                t = r.trim_start();
                break;
            }
        }
    }
    t.to_string()
}

/// Same, for option folders under `base` (e.g. design ideas made as a side task).
pub(crate) fn design_options_at(dir: &Path, base: &str) -> Vec<ApprovalOption> {
    DESIGN_OPTION_IDS
        .iter()
        .filter_map(|id| {
            let rel = format!("{base}/{id}");
            let d = dir.join(&rel);
            if !d.join("mockup.html").is_file() {
                return None;
            }
            let readme = std::fs::read_to_string(d.join("README.md")).unwrap_or_default();
            let mut lines = readme.lines().map(str::trim).filter(|l| !l.is_empty());
            let title = lines
                .next()
                .filter(|l| l.starts_with('#'))
                .map(|l| strip_option_letter(l.trim_start_matches('#').trim(), id))
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| format!("Phương án {}", id.to_uppercase()));
            let note: String = lines.collect::<Vec<_>>().join("\n").chars().take(600).collect();
            let images = ["desktop.png", "mobile.png"].iter().filter(|f| d.join(f).is_file()).map(|f| format!("{rel}/{f}")).collect();
            Some(ApprovalOption { id: id.to_string(), title, note, images, link: format!("{rel}/mockup.html") })
        })
        .collect()
}

/// Which option the answer picks ("Chọn phương án B …", "b", "B: …") and any extra feedback.
pub(crate) fn parse_choice(answer: &str, opts: &[ApprovalOption]) -> (Option<usize>, Option<String>) {
    // Byte offsets found in `lower` are only valid in the original when lowercasing kept the length.
    let lower = answer.trim().to_lowercase();
    let answer = if lower.len() == answer.trim().len() { answer.trim() } else { lower.as_str() };
    let find = |c: char| opts.iter().position(|o| o.id.starts_with(c));
    let letter_after = |s: &str| s.chars().next().filter(|c| c.is_ascii_alphabetic()).filter(|_| !s.chars().nth(1).is_some_and(|c| c.is_alphabetic()));
    let picked = lower
        .find("phương án")
        .and_then(|i| letter_after(lower[i + "phương án".len()..].trim_start()))
        .or_else(|| letter_after(&lower))
        .and_then(find);
    // Feedback: the text after "góp ý:", or whatever follows the first line when the answer was typed by hand.
    let fb = match lower.find("góp ý:") {
        Some(i) => answer.trim()[i + "góp ý:".len()..].trim().to_string(),
        None if picked.is_some() => {
            let mut parts = answer.trim().splitn(2, '\n');
            let first = parts.next().unwrap_or("");
            let rest = parts.next().unwrap_or("").trim().to_string();
            // "b: thêm màu xanh" → feedback after the separator on the first line.
            let inline = first.trim().get(1..).map(|s| s.trim_start_matches([':', '-', '.', ',', ' ']).trim()).unwrap_or("");
            if letter_after(&first.trim().to_lowercase()).is_some() && !inline.is_empty() { format!("{inline}\n{rest}").trim().to_string() } else { rest }
        }
        None => answer.trim().to_string(),
    };
    (picked, Some(fb).filter(|s| !s.is_empty()))
}

/// The chairman picks a direction from screenshots. Skipped / unclear answers keep the designer's pick (A).
async fn choose_design(store: &S, p: &Project, designer: &Employee, opts: Vec<ApprovalOption>, cancel: &CancellationToken) -> Result<(ApprovalOption, Option<String>), String> {
    let mut opts = opts;
    ensure_shots(p, &store.dir_of(p), &mut opts).await;
    store.set_runtime(&designer.id, EmpStatus::Waiting, "Chờ Chủ tịch chọn phương án thiết kế", Some(&p.id), None);
    if !autopilot(store, Some(&p.id)) {
        store.post_message(
            Some(&p.id),
            &designer.id,
            "chairman",
            format!("Em đã làm xong {} phương án thiết kế, mời anh/chị xem ảnh và chọn ở tab Duyệt — hoặc nhắn em, ví dụ \"chọn B, chữ to hơn chút\".", opts.len()),
            "chat",
        );
    }
    let list: String = opts.iter().map(|o| format!("- **{}. {}**", o.id.to_uppercase(), o.title)).collect::<Vec<_>>().join("\n");
    let title = format!("Chọn phương án thiết kế \"{}\"", p.name);
    // After a restart the card is still open: wait for it instead of asking twice.
    let open = store.with(|s| {
        s.approvals.iter().find(|a| a.project_id.as_deref() == Some(&p.id) && a.status == ApprovalStatus::Pending && a.title == title).map(|a| a.id.clone())
    });
    let a = match open {
        Some(id) => store.wait_approval(&id, cancel).await?,
        None => {
            store
                .request_choice(
                    Some(&p.id),
                    &designer.id,
                    ApprovalKind::Question,
                    title,
                    format!("Designer làm {} phương án giao diện, mời anh/chị xem ảnh và chọn một (có thể ghi thêm góp ý):\n{list}\n\nPhương án A là đề xuất của Designer.", opts.len()),
                    opts.clone(),
                    cancel,
                )
                .await?
        }
    };
    store.set_runtime(&designer.id, EmpStatus::Idle, "", None, None);
    let answer = a.answer.unwrap_or_default();
    let (picked, feedback) = parse_choice(&answer, &opts);
    let pick = opts[picked.unwrap_or(0)].clone();
    // A hand-typed answer without a letter is feedback on the recommended option.
    let msg = match (&feedback, picked, answer.trim().is_empty()) {
        (_, None, true) => format!("Chủ tịch để Designer tự quyết, dùng phương án {} \"{}\".", pick.id.to_uppercase(), pick.title),
        (Some(f), _, _) => format!("Chọn phương án {} \"{}\". Góp ý: {f}", pick.id.to_uppercase(), pick.title),
        (None, _, _) => format!("Chọn phương án {} \"{}\".", pick.id.to_uppercase(), pick.title),
    };
    store.post_message(Some(&p.id), "chairman", &designer.id, msg, "chat");
    Ok((pick, feedback))
}

/// The chairman (customer) looks at the mockup before anything is built; up to 2 rounds of changes.
async fn approve_design(store: &S, p: &Project, designer: &Employee, cancel: &CancellationToken) -> Result<(), String> {
    if !store.with(|s| s.settings.approve_design) || !store.dir_of(p).join("design").join("mockup.html").is_file() {
        return Ok(());
    }
    let url = format!("{}design/mockup.html", crate::config::preview_url(&p.id));
    for round in 1..=3 {
        store.set_runtime(&designer.id, EmpStatus::Waiting, "Chờ Chủ tịch duyệt thiết kế", Some(&p.id), None);
        let a = store
            .request_approval(
                Some(&p.id),
                &designer.id,
                ApprovalKind::Question,
                format!("Duyệt bản thiết kế giao diện \"{}\"", p.name),
                format!("Mời anh/chị xem bản mẫu giao diện: {url}\n\nĐồng ý: bấm Trả lời và ghi \"ok\". Muốn sửa: ghi góp ý cụ thể (màu sắc, bố cục, chữ, thêm / bớt gì…)."),
                cancel,
            )
            .await?;
        store.set_runtime(&designer.id, EmpStatus::Idle, "", None, None);
        let ans = a.answer.unwrap_or_default();
        let t = ans.trim().to_lowercase();
        if a.status == ApprovalStatus::Denied && t.is_empty() {
            // Chairman dismissed / skipped without feedback (or server restarted): keep current design instead of wasting redesign rounds.
            store.post_message(Some(&p.id), &designer.id, "all", "Chủ tịch bỏ qua duyệt thiết kế (không có góp ý sửa), đội tiếp tục với bản hiện tại.", "chat");
            return Ok(());
        }
        let ok = a.status == ApprovalStatus::Approved
            || t.is_empty() && a.status == ApprovalStatus::Answered
            || ["ok", "oke", "okay", "đồng ý", "duyệt", "được", "đẹp", "chốt", "yes"].iter().any(|w| t == *w || t.starts_with(&format!("{w} ")) || t.starts_with(&format!("{w},")) || t.starts_with(&format!("{w}.")));
        if ok {
            store.post_message(Some(&p.id), "chairman", &designer.id, "Duyệt bản thiết kế.", "chat");
            return Ok(());
        }
        if round == 3 {
            break;
        }
        let feedback = if t.is_empty() { "Chủ tịch chưa ưng bản thiết kế, hãy đề xuất hướng khác đẹp và chuyên nghiệp hơn.".to_string() } else { ans.trim().to_string() };
        store.post_message(Some(&p.id), "chairman", &designer.id, format!("Góp ý thiết kế: {feedback}"), "chat");
        let task = format!(
            "Chủ tịch (khách hàng) góp ý bản thiết kế:\n{feedback}\n\nSửa docs/DESIGN.md, design/tokens.css và design/mockup.html theo góp ý (giữ những gì khách không nhắc tới). Mở lại {url} bằng trình duyệt nếu có công cụ để kiểm tra. Xong gọi finish với tóm tắt đã sửa gì."
        );
        agent::run(store, Job { employee_id: designer.id.clone(), project_id: p.id.clone(), phase: "design", ticket_id: None, case_id: None, task }, cancel).await?;
    }
    store.post_message(Some(&p.id), &designer.id, "chairman", "Đã sửa thiết kế theo góp ý 2 lần; đội tiếp tục với bản hiện tại, anh/chị có thể yêu cầu sửa thêm khi duyệt sản phẩm.", "chat");
    Ok(())
}

async fn phase_architecture(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let Some(lead) = first(store, &[Role::Architect]) else { return Ok(()) };
    if let Some(from) = first(store, &[Role::Designer, Role::Pm, Role::Ba, Role::Ceo]) {
        handoff(store, &p.id, &from.id, &lead.id, "Yêu cầu và thiết kế đã sẵn sàng, nhờ anh/chị thiết kế kiến trúc.");
    }
    if is_standard(p) {
        let task = format!(
            "Đọc docs/PLAN.md, docs/REQUIREMENTS.md (và docs/DESIGN.md nếu có).\n\n{}\n\nDỰ ÁN CỠ VỪA: viết docs/ARCHITECTURE.md NGẮN (1–2 trang): công nghệ và phiên bản (đúng PLAN.md; chọn giải pháp ĐƠN GIẢN NHẤT đáp ứng yêu cầu — chỉ có máy chủ / cơ sở dữ liệu khi yêu cầu thật sự cần), cây thư mục theo chuẩn trên, bảng trang / module và trách nhiệm, cách chạy / test / build. KHÔNG viết ADR, tài liệu từng module hay mô hình dữ liệu riêng. Nếu chưa có khung code thì tạo MỘT ticket kỹ thuật \"Khởi tạo khung dự án\" (README, .gitignore, cấu trúc thư mục, lệnh test / build). Xong gọi finish.{}",
            crate::config::REPO_STANDARD,
            revision_note(p)
        );
        agent::run(store, Job { employee_id: lead.id.clone(), project_id: p.id.clone(), phase: "architecture", ticket_id: None, case_id: None, task }, cancel).await?;
        return Ok(());
    }
    let task = format!(
        "Đọc docs/PLAN.md, docs/DECISIONS.md, docs/REQUIREMENTS.md (và docs/DESIGN.md nếu có).\n\n{}\n\nCông nghệ: dùng đúng công nghệ đã chốt trong PLAN.md (lựa chọn của Chủ tịch). Tuân thủ docs/CONSTITUTION.md và ghi mục \"Kiểm tra Constitution\" (từng nguyên tắc: đạt / lý do) trong ARCHITECTURE.md. Viết docs/ARCHITECTURE.md: công nghệ và phiên bản, cây thư mục cụ thể của dự án này theo đúng chuẩn trên, BẢNG DANH SÁCH TRANG / MODULE (tên, trách nhiệm, API công khai, phụ thuộc), quy tắc phụ thuộc, quy ước code, cách chạy / test / build, và hướng dẫn \"thêm một trang / module mới\" từng bước. Viết docs/modules/<ten-module>.md cho từng module. Ghi mỗi quyết định kiến trúc quan trọng thành một ADR trong docs/adr/. Tạo ticket kỹ thuật đầu tiên \"Khởi tạo khung dự án theo chuẩn công ty\" (thư mục, README, CHANGELOG, .gitignore, .editorconfig, CI) và các ticket kỹ thuật khác nếu cần. Xong gọi finish.{}",
        crate::config::REPO_STANDARD,
        revision_note(p)
    );
    agent::run(store, Job { employee_id: lead.id.clone(), project_id: p.id.clone(), phase: "architecture", ticket_id: None, case_id: None, task }, cancel).await?;
    phase_data(store, p, &lead, cancel).await?;
    analyze(store, p, cancel).await
}

/// The Data Engineer designs the data layer right after the architecture (skipped if there is none).
async fn phase_data(store: &S, p: &Project, lead: &Employee, cancel: &CancellationToken) -> Result<(), String> {
    let Some(de) = first(store, &[Role::DataEngineer]) else { return Ok(()) };
    handoff(store, &p.id, &lead.id, &de.id, "Kiến trúc đã xong, nhờ thiết kế tầng dữ liệu.");
    let task = format!(
        "Đọc docs/PLAN.md, docs/DECISIONS.md, docs/REQUIREMENTS.md, docs/ARCHITECTURE.md.\n\n{}\n\nViệc của bạn (Data Engineer): viết docs/DATA.md gồm\n- Mô hình dữ liệu: từng thực thể, trường, kiểu, bắt buộc/tùy chọn, ràng buộc, quan hệ, ví dụ JSON.\n- Lưu trữ: ở đâu, khóa, giới hạn dung lượng, xử lý khi dữ liệu hỏng.\n- Phiên bản schema và cách chuyển đổi dữ liệu cũ khi nâng cấp (migration).\n- Sao lưu / khôi phục / xuất dữ liệu; bảo vệ dữ liệu cá nhân.\n- Các chỉ số sản phẩm nên đo (nếu phù hợp) và cách tính.\nTạo data/schema/ (định nghĩa schema, vd. JSON Schema), data/migrations/ (nếu cần) và data/seeds/ (dữ liệu mẫu). Nếu phần dữ liệu cần code thì tạo ticket kỹ thuật bằng create_ticket với tiêu chí nghiệm thu rõ ràng. Không viết code giao diện. Xong gọi finish.{}",
        crate::config::REPO_STANDARD,
        revision_note(p)
    );
    agent::run(store, Job { employee_id: de.id.clone(), project_id: p.id.clone(), phase: "architecture", ticket_id: None, case_id: None, task }, cancel).await?;
    Ok(())
}

async fn phase_build(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    if is_fast(p) {
        return fast_build(store, p, cancel).await;
    }
    let devs = team_staff(store, Role::Dev);
    if devs.is_empty() {
        return Err("Công ty cần ít nhất 1 Dev đang làm việc".into());
    }
    // Resume cleanly: only tickets interrupted during Dev coding restart as Todo;
    // tickets already in Review or Testing keep their stage so Dev/Reviewer work is not repeated.
    for t in store.tickets_of(&p.id) {
        if t.status == TicketStatus::InProgress {
            store.update_ticket(&t.id, |t| t.status = TicketStatus::Todo);
        }
    }
    let todo = store
        .tickets_of(&p.id)
        .iter()
        .filter(|t| matches!(t.status, TicketStatus::Todo | TicketStatus::Review | TicketStatus::Testing))
        .count();

    // Daily standup led by the PM (only once per project build phase, skipped on pause/resume).
    let already_had_daily = store.with(|s| {
        s.messages
            .iter()
            .any(|m| m.project_id.as_deref() == Some(&p.id) && m.text.starts_with("Daily:"))
    });
    if !already_had_daily && !is_standard(p) {
        if let Some(pm) = first(store, &[Role::Pm]) {
            let mut team: Vec<String> = vec![pm.id.clone()];
            for r in [Role::Dev, Role::Reviewer, Role::Tester, Role::Designer] {
                team.extend(team_staff(store, r).into_iter().map(|e| e.id));
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
    }

    run_dev_workers(store, &p.id, cancel).await
}

/// How every Dev ends a job: what the next person (Reviewer / Tester) needs, so nothing is lost in the hand-over.
const HANDOFF_FORMAT: &str = " Phần tóm tắt khi finish là BIÊN BẢN BÀN GIAO cho người kiểm tra tiếp theo, gồm: (a) tệp đã tạo / sửa; (b) cách kiểm tra từng tiêu chí (lệnh chạy, địa chỉ trang, bấm gì → thấy gì); (c) điều chưa làm, giả định, rủi ro.";

pub(crate) fn is_fast(p: &Project) -> bool {
    p.mode == "fast"
}

/// Medium project (M): small team, short documents, one final QA.
pub(crate) fn is_standard(p: &Project) -> bool {
    p.mode == "standard"
}

/// S or M: the team is deliberately small and there are no company-wide ceremonies.
pub(crate) fn is_small(p: &Project) -> bool {
    is_fast(p) || is_standard(p)
}

/// A change request on a project that already has a backlog (not a first run).
fn is_revision(store: &S, p: &Project) -> bool {
    !p.feedback.is_empty() && !store.tickets_of(&p.id).is_empty()
}

/// Impact analysis of a change request (what a real PM does): the existing backlog is updated, cancelled
/// or extended — never duplicated — and the steps to redo are decided. Returns the scope ("design", "arch", "code").
async fn phase_change(store: &S, p: &Project, cancel: &CancellationToken) -> Result<String, String> {
    let who = first(store, &[Role::Pm, Role::Architect, Role::Ceo]).ok_or("Cần PM, Tech Lead hoặc CEO để phân tích yêu cầu sửa")?;
    let n = p.feedback.len();
    let fb = p.feedback.last().cloned().unwrap_or_default();
    store.post_message(Some(&p.id), "chairman", &who.id, format!("📋 Yêu cầu sửa đổi (vòng {n}): {fb}"), "handoff");
    let file = format!("docs/CHANGES-sua-{n}.md");
    let task = format!(
        "YÊU CẦU SỬA ĐỔI vòng {n} của khách (Chủ tịch):\n{fb}\n\nYêu cầu gốc của dự án:\n{}\n\nSản phẩm đã có trong thư mục dự án. Các ticket hiện có:\n{}\nViệc của bạn là PHÂN TÍCH ẢNH HƯỞNG như PM của công ty phần mềm thật — KHÔNG lập kế hoạch lại từ đầu, không họp:\n1. Xem nhanh đúng phần liên quan (docs/PLAN.md, docs/REQUIREMENTS.md, design/, cấu trúc code) — không đọc hết mọi tài liệu.\n2. Xác định yêu cầu sửa chạm tới đâu: chỉ giao diện / thêm-bớt-sửa tính năng / dữ liệu / kiến trúc.\n3. Sắp lại backlog trên TICKET CŨ: ticket chưa làm không còn phù hợp → cancel_ticket; ticket cần đổi nội dung (kể cả ticket đã xong nay phải sửa) → update_ticket (ticket được mở lại để làm); chỉ create_ticket cho phần việc thật sự MỚI; set_priority theo thứ tự làm. KHÔNG tạo ticket trùng nội dung với ticket đã có. Không thêm việc khách không yêu cầu.\n4. Ghi {file} ngắn gọn: yêu cầu sửa, phạm vi ảnh hưởng, ticket đã hủy / sửa / thêm, rủi ro.\n5. Gọi finish; dòng ĐẦU của tóm tắt BẮT BUỘC có dạng \"PHẠM VI: …\" với một hoặc nhiều từ: thiet-ke (phải sửa bản thiết kế giao diện trước khi code), kien-truc (phải đổi công nghệ / cấu trúc dự án / dữ liệu), code (chỉ cần sửa code). Ví dụ: \"PHẠM VI: thiet-ke, code\".",
        p.request,
        ticket_list(store, &p.id)
    );
    let r = agent::run(store, Job { employee_id: who.id.clone(), project_id: p.id.clone(), phase: "change", ticket_id: None, case_id: None, task }, cancel).await?;
    let summary = summary_of(&r.outcome);
    let scope = change_scope_of(&summary);
    store.update_project(&p.id, |x| x.change_scope = scope.clone());
    store.post_message(Some(&p.id), &who.id, "all", format!("Phân tích yêu cầu sửa: {}", short_text(&summary, 500)), "chat");
    Ok(scope)
}

/// Reads "PHẠM VI: thiet-ke, code" from the analysis summary → "design,code" (code is always redone).
fn change_scope_of(summary: &str) -> String {
    let head = summary.lines().find(|l| l.to_lowercase().contains("phạm vi")).unwrap_or("").to_lowercase();
    let mut scope = vec![];
    if head.contains("thiet-ke") || head.contains("thiết kế") || head.contains("thiet ke") {
        scope.push("design");
    }
    if head.contains("kien-truc") || head.contains("kiến trúc") || head.contains("kien truc") {
        scope.push("arch");
    }
    scope.push("code");
    scope.join(",")
}

/// Medium project before hand-over: one final QA (functional + visual), then the automatic browser gate.
async fn release_standard(store: &S, p: &Project, cancel: &CancellationToken) -> Result<bool, String> {
    if p.qa_rounds < MAX_QA_ROUNDS && final_qa(store, p, cancel).await? {
        return Ok(true);
    }
    ship_gate(store, p, cancel).await
}

/// Who builds a small project alone: the Tech Lead (strongest model), else a Dev.
fn fast_builder(store: &S) -> Option<Employee> {
    first(store, &[Role::Architect, Role::Dev])
}

/// The project's size: the chairman's choice ("fast" / "standard" / "full"); a new project without one is medium.
/// The fast lane is announced once so the chairman knows who is building it.
fn decide_mode(store: &S, p: &Project) -> String {
    if p.mode.is_empty() {
        store.update_project(&p.id, |x| x.mode = "standard".into());
        return "standard".into();
    }
    if p.mode == "fast" && p.feedback.is_empty() && store.tickets_of(&p.id).is_empty() {
        let who = fast_builder(store).map(|e| format!("{} ({})", e.name, crate::config::role_label(e.role))).unwrap_or_else(|| "một kỹ sư".into());
        let from = first(store, &[Role::Ceo]).map(|e| e.id).unwrap_or_else(|| "system".into());
        store.post_message(Some(&p.id), &from, "chairman", format!("⚡ Làm theo CHẾ ĐỘ NHANH: {who} làm liền một mạch từ đầu đến cuối, máy tự kiểm tra trên trình duyệt rồi Tester dùng thử như người dùng — không họp, không đặc tả dài."), "chat");
    }
    p.mode.clone()
}

/// The fast lane's single piece of work: the whole request (or this round of changes) as one ticket.
fn fast_plan(store: &S, p: &Project) -> Result<(), String> {
    let builder = fast_builder(store).ok_or("Công ty cần ít nhất 1 Tech Lead hoặc Dev đang làm việc")?;
    let open = store.tickets_of(&p.id).iter().any(|t| !matches!(t.status, TicketStatus::Done | TicketStatus::Blocked | TicketStatus::Cancelled));
    if !open {
        let (title, description) = match p.feedback.last() {
            Some(fb) => (format!("[Sửa đổi {}] {}", p.feedback.len(), short_text(fb, 70)), format!("YÊU CẦU SỬA của khách (vòng {}):\n{fb}\n\nYêu cầu gốc của dự án:\n{}\n\nSản phẩm cũ đã có trong thư mục dự án: chỉ sửa phần được yêu cầu, không làm hỏng phần đang chạy.", p.feedback.len(), p.request)),
            None => (format!("Làm trọn sản phẩm: {}", short_text(&p.name, 60)), format!("Yêu cầu của khách:\n{}", p.request)),
        };
        let seq = store.update_project(&p.id, |p| {
            p.ticket_seq += 1;
            p.ticket_seq
        });
        store.insert_ticket(Ticket {
            id: crate::store::new_id(),
            project_id: p.id.clone(),
            key: format!("T-{}", seq.unwrap_or(1)),
            title,
            description,
            acceptance: vec![
                "Đáp ứng đầy đủ từng ý trong yêu cầu của khách".into(),
                "Mở bản xem thử là dùng được ngay: không lỗi JavaScript, không tệp 404".into(),
                "Giao diện đẹp, hiện đại; hiển thị tốt ở máy tính 1280px và điện thoại 375px, không tràn ngang".into(),
                "Mọi nút / liên kết nhìn rõ và bấm được".into(),
                "Có README.md ngắn: sản phẩm là gì, cách mở / chạy".into(),
            ],
            kind: "feature".into(),
            status: TicketStatus::Todo,
            assignee: Some(builder.id.clone()),
            created_by: "system".into(),
            bounces: 0,
            notes: vec![],
            priority: 1,
            created_at: now_ms(),
            updated_at: now_ms(),
        });
    }
    if let Some(ceo) = first(store, &[Role::Ceo]).filter(|c| c.id != builder.id) {
        handoff(store, &p.id, &ceo.id, &builder.id, "Chế độ làm nhanh: em làm trọn sản phẩm theo yêu cầu của khách, xong bàn giao Tester dùng thử.");
    }
    Ok(())
}

/// One engineer works through the open tickets (the product, then fixes found by the checks).
async fn fast_build(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let builder = fast_builder(store).ok_or("Công ty cần ít nhất 1 Tech Lead hoặc Dev đang làm việc")?;
    for t in store.tickets_of(&p.id) {
        if t.status == TicketStatus::InProgress {
            store.update_ticket(&t.id, |t| t.status = TicketStatus::Todo);
        }
    }
    loop {
        if cancel.is_cancelled() {
            return Err(ABORTED.into());
        }
        let next = store.with(|s| {
            let mut c: Vec<&Ticket> = s.tickets.iter().filter(|t| t.project_id == p.id && matches!(t.status, TicketStatus::Todo | TicketStatus::Review | TicketStatus::Testing)).collect();
            c.sort_by_key(|t| ticket_pick_key(t));
            c.first().map(|t| (t.id.clone(), t.status, t.assignee.clone()))
        });
        let Some((tid, stage, assignee)) = next else { return Ok(()) };
        let who = assignee.filter(|a| store.employee(a).is_some_and(|e| e.enabled)).unwrap_or_else(|| builder.id.clone());
        store.update_ticket(&tid, |t| {
            if t.status == TicketStatus::Todo {
                t.status = TicketStatus::InProgress;
            }
            t.assignee = Some(who.clone());
        });
        process_ticket(store, &p.id, &who, &tid, stage, cancel).await?;
    }
}

fn fast_build_task(pid: &str) -> String {
    format!(
        "\nCHẾ ĐỘ LÀM NHANH: bạn làm MỘT MÌNH trọn việc này từ đầu đến cuối như kỹ sư senior của công ty hàng đầu — không có họp, không có đặc tả / kiến trúc dài, ĐỪNG viết tài liệu thừa.\n1. Đọc nhanh thư mục dự án (list_files). Chọn công nghệ ĐƠN GIẢN NHẤT đáp ứng yêu cầu (ưu tiên HTML/CSS/JS thuần; chỉ dùng Vite / framework khi thật cần), cấu trúc thư mục gọn.\n2. Viết code chạy được ngay; giao diện đẹp, hiện đại, dễ dùng (tự thiết kế; cần tham khảo thì web_search; tài nguyên chuẩn như icon / quân cờ / cờ thì tải bản gốc bằng download_file, không tự vẽ).\n3. Mở bản xem thử {} bằng trình duyệt ở 1280px và 375px, bấm thử mọi chức năng, xem console, chụp màn hình tự soi; sửa tới khi chạy đúng và đẹp.\n4. README.md ngắn: sản phẩm là gì, cách mở / chạy.\nXong gọi finish.{HANDOFF_FORMAT}",
        crate::config::preview_url(pid)
    )
}

/// Files changed but not committed yet (= the work of the ticket in progress).
async fn changed_files(dir: &Path) -> String {
    if !dir.join(".git").exists() {
        return String::new();
    }
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(["status", "-s", "--untracked-files=all"]).current_dir(dir).env("GIT_TERMINAL_PROMPT", "0").kill_on_drop(true);
    if let Some(parent) = dir.parent() {
        cmd.env("GIT_CEILING_DIRECTORIES", parent);
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(8), cmd.output()).await else { return String::new() };
    let s = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = s.lines().filter(|l| !l.contains(".playwright-mcp") && !l.contains("node_modules")).collect();
    let mut out = lines.iter().take(40).copied().collect::<Vec<_>>().join("\n");
    if lines.len() > 40 {
        out.push_str(&format!("\n… và {} tệp khác", lines.len() - 40));
    }
    out
}

/// Sort key for picking the next workable ticket: finish Testing/Review tickets first so resumes never redo prior stages.
fn ticket_pick_key(t: &Ticket) -> (u8, u8, u64) {
    let stage_rank = match t.status {
        TicketStatus::Testing => 0,
        TicketStatus::Review => 1,
        _ => 2,
    };
    (stage_rank, t.priority, t.created_at)
}

/// Processes open tickets of a project one at a time in priority/dependency order,
/// rotating across enabled Devs. Serialising per project prevents two Devs sharing one
/// working tree from clobbering each other's files or committing half-finished WIP together.
pub(crate) async fn run_dev_workers(store: &S, pid: &str, cancel: &CancellationToken) -> Result<(), String> {
    let devs = team_staff(store, Role::Dev);
    if devs.is_empty() {
        return Err("Công ty cần ít nhất 1 Dev đang làm việc".into());
    }
    let mut rr: usize = 0;
    loop {
        if cancel.is_cancelled() {
            return Err(ABORTED.into());
        }
        let devs = team_staff(store, Role::Dev);
        if devs.is_empty() {
            return Err("Công ty cần ít nhất 1 Dev đang làm việc".into());
        }
        let picked: Option<(String, String, TicketStatus)> = store.with(|s| {
            let mut cands: Vec<&Ticket> = s
                .tickets
                .iter()
                .filter(|t| t.project_id == pid && matches!(t.status, TicketStatus::Todo | TicketStatus::Review | TicketStatus::Testing))
                .collect();
            cands.sort_by_key(|t| ticket_pick_key(t));
            let t = cands.first()?;
            let dev_id = t
                .assignee
                .as_deref()
                .filter(|a| devs.iter().any(|d| d.id == *a))
                .map(str::to_string)
                .unwrap_or_else(|| devs[rr % devs.len()].id.clone());
            Some((t.id.clone(), dev_id, t.status))
        });
        let Some((tid, dev_id, start_stage)) = picked else {
            return Ok(());
        };
        rr = rr.wrapping_add(1);
        store.update_ticket(&tid, |t| {
            if t.status == TicketStatus::Todo {
                t.status = TicketStatus::InProgress;
            }
            if t.assignee.is_none() {
                t.assignee = Some(dev_id.clone());
            }
        });
        process_ticket(store, pid, &dev_id, &tid, start_stage, cancel).await?;
    }
}

fn block_ticket_refusal(store: &S, pid: &str, tid: &str, key: &str, title: &str, role_who: &str, err: &str) {
    let reason = format!("Bị chặn do model từ chối yêu cầu tại bước {role_who}: {}", short_text(err, 240));
    store.update_ticket(tid, |t| {
        t.status = TicketStatus::Blocked;
        t.notes.push(reason.clone());
    });
    if let Some(p) = store.project(pid) {
        crate::harness::record_entry(&store.dir_of(&p), &format!("{key} · Blocked ({role_who})"), &reason);
    }
    store.post_message(
        Some(pid),
        "system",
        "chairman",
        format!("Ticket {key} ({title}) đã bị đánh dấu Blocked — {reason}. Các ticket khác vẫn tiếp tục."),
        "system",
    );
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

fn last_dev_summary(t: &Ticket) -> String {
    t.notes
        .iter()
        .rev()
        .find(|n| n.contains("(Dev):"))
        .cloned()
        .unwrap_or_else(|| format!("Đã hoàn thành {} ({}), tiếp tục bước kiểm tra sau tạm dừng.", t.key, t.title))
}

async fn process_ticket(store: &S, pid: &str, dev_id: &str, tid: &str, mut start_stage: TicketStatus, cancel: &CancellationToken) -> Result<(), String> {
    let max_bounces = store.with(|s| s.settings.max_bounces);
    let project_dir = store.project(pid).map(|p| store.dir_of(&p));
    let fast = store.project(pid).is_some_and(|p| is_fast(&p));
    let standard = store.project(pid).is_some_and(|p| is_standard(&p));
    loop {
        let t = store.ticket(tid).ok_or("Ticket không tồn tại")?;
        let dev_summary = if matches!(start_stage, TicketStatus::Review | TicketStatus::Testing) {
            last_dev_summary(&t)
        } else {
            store.update_ticket(tid, |t| t.status = TicketStatus::InProgress);
            let task = if fast {
                format!("{}\n{}", ticket_brief(&t), fast_build_task(pid))
            } else {
                format!(
                    "{}\nDựa vào Bộ nhớ tiến độ (docs/PROGRESS.md) và CHỈ đọc đúng các tệp/đoạn code liên quan trực tiếp tới ticket này (KHÔNG đọc lại toàn bộ docs/*.md hay cả dự án từ đầu khi tiêu chí đã rõ). Viết/sửa code hoàn thành ticket theo đúng cây thư mục trong docs/ARCHITECTURE.md và tiêu chí nghiệm thu, tự chạy test kiểm chứng bằng run_command, rồi gọi finish.{HANDOFF_FORMAT}",
                    ticket_brief(&t)
                )
            };
            let r = match agent::run(store, Job { employee_id: dev_id.into(), project_id: pid.into(), phase: "implement", ticket_id: Some(tid.into()), case_id: None, task }, cancel).await {
                Ok(r) => r,
                Err(e) if agent::is_refusal_err(&e) => {
                    block_ticket_refusal(store, pid, tid, &t.key, &t.title, "Dev", &e);
                    return Ok(());
                }
                Err(e) => return Err(e),
            };
            let s = summary_of(&r.outcome);
            let dev_name = store.employee(dev_id).map(|e| e.name).unwrap_or_default();
            store.update_ticket(tid, |t| t.notes.push(format!("{dev_name} (Dev): {s}")));
            if let Some(dir) = project_dir.as_deref() {
                crate::harness::record_entry(dir, &format!("{} · Dev bàn giao ({dev_name})", t.key), &short_text(&s, 700));
            }
            s
        };

        let bounce_focus = if t.bounces >= 1 {
            format!(
                "\nLƯU Ý VÒNG KIỂM TRA LẠI (lần {}): Tập trung xác minh các lỗi đã báo ở vòng trước đã được sửa đúng và không gây hồi quy; KHÔNG mở rộng phạm vi sang các ca giả lập cực đoan ngoài tiêu chí nghiệm thu.",
                t.bounces + 1
            )
        } else {
            String::new()
        };

        // What changed (uncommitted = this ticket's work): the next person starts from facts, not a guess.
        let changes = match project_dir.as_deref() {
            Some(d) => changed_files(d).await,
            None => String::new(),
        };
        let dev_summary = if changes.is_empty() { dev_summary } else { format!("{dev_summary}\nTệp thay đổi (git status -s):\n{changes}") };

        // Optional code review (skipped on resume if ticket was already in Testing, and in the fast lane).
        if start_stage != TicketStatus::Testing && !fast {
            if let Some(rev) = first(store, &[Role::Reviewer]) {
                store.update_ticket(tid, |t| t.status = TicketStatus::Review);
                handoff(store, pid, dev_id, &rev.id, format!("Nhờ review {}: {}", t.key, t.title));
                let task = format!(
                    "{}\nDev báo: {dev_summary}{bounce_focus}\n\nƯU TIÊN chạy `git status -s` và `git diff` (hoặc `git show --stat HEAD`) để xem ĐÚNG phần vừa thay đổi, KHÔNG đọc lại toàn bộ các tệp dài hàng nghìn dòng từ đầu đến cuối; chỉ đọc thêm đúng đoạn hàm liên quan khi cần. Kiểm tra đúng tiêu chí nghiệm thu, an toàn, cấu trúc, rồi BẮT BUỘC gọi approve_code nếu đạt, hoặc request_changes nếu có lỗi thật cần sửa.",
                    ticket_brief(&t)
                );
                let r = match agent::run(store, Job { employee_id: rev.id.clone(), project_id: pid.into(), phase: "review", ticket_id: Some(tid.into()), case_id: None, task }, cancel).await {
                    Ok(r) => r,
                    Err(e) if agent::is_refusal_err(&e) => {
                        block_ticket_refusal(store, pid, tid, &t.key, &t.title, "Reviewer", &e);
                        return Ok(());
                    }
                    Err(e) => return Err(e),
                };
                store.kpi(&rev.id, |k| k.reviews += 1);
                match r.outcome {
                    Outcome::Approve(comment) => {
                        let note = format!("{} (Reviewer): Duyệt. {comment}", rev.name);
                        store.update_ticket(tid, |t| t.notes.push(note));
                    }
                    Outcome::Changes(c) => {
                        store.kpi(&rev.id, |k| k.changes_requested += 1);
                        store.kpi(dev_id, |k| k.tickets_returned += 1);
                        handoff(store, pid, &rev.id, dev_id, format!("{} cần sửa: {c}", t.key));
                        if let Some(dir) = project_dir.as_deref() {
                            crate::harness::record_entry(dir, &format!("{} · Reviewer yêu cầu sửa ({})", t.key, rev.name), &short_text(&c, 700));
                        }
                        if bounce(store, tid, format!("{} (Reviewer): {c}", rev.name), max_bounces) {
                            return Ok(());
                        }
                        start_stage = TicketStatus::InProgress;
                        continue;
                    }
                    other => {
                        // Never treat an invalid/fallback outcome as Approve.
                        let msg = format!("{} (Reviewer) kết thúc không gọi approve_code/request_changes: {}", rev.name, short_text(&summary_of(&other), 260));
                        store.log_activity(&rev.id, Some(pid), "error", &msg);
                        if bounce(store, tid, msg, max_bounces) {
                            return Ok(());
                        }
                        start_stage = TicketStatus::InProgress;
                        continue;
                    }
                }
            }
        }

        // Optional QA.
        let Some(tester) = first(store, &[Role::Tester]) else {
            store.update_ticket(tid, |t| t.status = TicketStatus::Done);
            store.kpi(dev_id, |k| k.tickets_done += 1);
            crate::gitflow::commit(store, pid, &format!("{}: {}\n\n{}", t.key, t.title, short_text(&dev_summary, 600))).await;
            return Ok(());
        };
        store.update_ticket(tid, |t| t.status = TicketStatus::Testing);
        handoff(store, pid, dev_id, &tester.id, format!("Nhờ test {}: {}", t.key, t.title));
        let browser_step = if project_dir.as_deref().map(has_preview).unwrap_or(false) {
            format!(" test như người dùng trên trình duyệt thật ở bản xem thử {} (desktop và 375px, xem lỗi console) nếu có công cụ trình duyệt;", crate::config::preview_url(pid))
        } else {
            String::new()
        };
        let task = if fast {
            format!(
                "{}\nNgười làm bàn giao: {dev_summary}{bounce_focus}\n\nCHẾ ĐỘ LÀM NHANH (dự án nhỏ): kiểm tra như NGƯỜI DÙNG THẬT, không cần bộ tài liệu QA đầy đủ. Mở bản xem thử {} bằng trình duyệt ở 1280px và 375px, đi qua TỪNG Ý trong yêu cầu của khách (bấm thử mọi nút, nhập thử, xem console có lỗi không), chụp màn hình để tự soi giao diện (chữ khó đọc, lệch, tràn, hình méo). Ghi docs/qa/QA-REPORT.md ngắn: đã thử gì, kết quả, lỗi còn lại. BẮT BUỘC gọi pass_ticket nếu người dùng dùng được trọn vẹn, hoặc fail_ticket kèm từng lỗi cụ thể (bước làm, thấy gì, mong đợi gì).",
                ticket_brief(&t),
                crate::config::preview_url(pid)
            )
        } else if standard {
            format!(
                "{}\nDev báo: {dev_summary}{bounce_focus}\n\nKiểm tra ticket theo từng tiêu chí nghiệm thu: chạy NGAY bộ test hiện có;{browser_step} xem `git diff` đúng phạm vi ticket (không đọc lại cả dự án). Thêm test tự động cho ca quan trọng nếu còn thiếu. Ghi ngắn kết quả của ticket này vào docs/qa/QA-REPORT.md — dự án cỡ vừa KHÔNG cần TEST-CASES / RTM / CSV. BẮT BUỘC gọi pass_ticket nếu đạt hết tiêu chí, hoặc fail_ticket kèm từng lỗi cụ thể (bước làm, thấy gì, mong đợi gì).",
                ticket_brief(&t),
            )
        } else { format!(
            "{}\nDev báo: {dev_summary}{bounce_focus}\n\nKiểm tra ticket theo từng tiêu chí nghiệm thu, đúng kỹ năng Tester của bạn: chạy NGAY bộ test hiện có;{browser_step} xem `git show --stat` / `git diff` đúng phạm vi ticket (KHÔNG đọc lại từ đầu đến cuối các tệp dài > 500 dòng khi test đã xanh và tiêu chí đã rõ); bổ sung/giữ lại test tự động cho ca biên nếu còn thiếu (tên test ghi mã TC); cập nhật bộ tài liệu QA trong docs/qa/ CHỈ cho phần của ticket này (các dòng case của FR trong ticket ở TEST-CASES.md với kết quả thực tế + trạng thái, các dòng FR đó trong RTM.md, bug mới / retest trong BUGS.md; chưa có TEST-PLAN.md thì viết). BẮT BUỘC gọi pass_ticket ngay khi đạt hết tiêu chí, hoặc fail_ticket kèm mô tả lỗi cụ thể nếu chưa đạt.",
            ticket_brief(&t),
        ) };
        let r = match agent::run(store, Job { employee_id: tester.id.clone(), project_id: pid.into(), phase: "test", ticket_id: Some(tid.into()), case_id: None, task }, cancel).await {
            Ok(r) => r,
            Err(e) if agent::is_refusal_err(&e) => {
                block_ticket_refusal(store, pid, tid, &t.key, &t.title, "Tester", &e);
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        match r.outcome {
            Outcome::Pass(summary) => {
                store.update_ticket(tid, |t| {
                    t.status = TicketStatus::Done;
                    t.notes.push(format!("{} (Tester): Đạt. {summary}", tester.name));
                });
                store.kpi(dev_id, |k| k.tickets_done += 1);
                store.post_message(Some(pid), &tester.id, dev_id, format!("{} đạt ✔", t.key), "chat");
                if let Some(dir) = project_dir.as_deref() {
                    crate::harness::record_entry(dir, &format!("{} · ĐẠT ✔ ({})", t.key, tester.name), &short_text(&summary, 500));
                }
                crate::gitflow::commit(store, pid, &format!("{}: {}\n\n{}", t.key, t.title, short_text(&dev_summary, 600))).await;
                return Ok(());
            }
            Outcome::Fail(report) => {
                store.kpi(&tester.id, |k| k.bugs_found += 1);
                store.kpi(dev_id, |k| k.tickets_returned += 1);
                handoff(store, pid, &tester.id, dev_id, format!("{} chưa đạt: {report}", t.key));
                if let Some(dir) = project_dir.as_deref() {
                    crate::harness::record_entry(dir, &format!("{} · QA báo lỗi ({})", t.key, tester.name), &short_text(&report, 700));
                }
                if bounce(store, tid, format!("{} (Tester): {report}", tester.name), max_bounces) {
                    return Ok(());
                }
                start_stage = TicketStatus::InProgress;
            }
            other => {
                // Never treat an invalid/fallback outcome as Pass.
                let msg = format!("{} (Tester) kết thúc không gọi pass_ticket/fail_ticket: {}", tester.name, short_text(&summary_of(&other), 260));
                store.log_activity(&tester.id, Some(pid), "error", &msg);
                if bounce(store, tid, msg, max_bounces) {
                    return Ok(());
                }
                start_stage = TicketStatus::InProgress;
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
    if p.design_reviews < MAX_DESIGN_REVIEWS && design_review(store, p, cancel).await? {
        return Ok(true); // UI issues → back to Build
    }
    if p.security_reviews < 1 && security_review(store, p, cancel).await? {
        return Ok(true); // vulnerabilities → back to Build
    }
    if release_check(store, p, p.release_rounds < MAX_RELEASE_ROUNDS, cancel).await? {
        return Ok(true);
    }
    if p.qa_rounds < MAX_QA_ROUNDS && final_qa(store, p, cancel).await? {
        return Ok(true); // regressions → back to Build
    }
    if ship_gate(store, p, cancel).await? {
        return Ok(true); // blocked tickets / problems a user would see → back to Build
    }
    write_specification(store, p, cancel).await?;
    Ok(false)
}

const MAX_GATE_ROUNDS: u32 = 2;

/// Nothing is handed over "done" while a ticket is blocked or a browser finds problems a user would
/// hit. The work goes back to Build (twice at most); after that the chairman decides.
async fn ship_gate(store: &S, p: &Project, cancel: &CancellationToken) -> Result<bool, String> {
    let blocked: Vec<(String, String)> = store.with(|s| {
        s.tickets.iter().filter(|t| t.project_id == p.id && t.status == TicketStatus::Blocked).map(|t| (t.id.clone(), format!("{}: {}", t.key, t.title))).collect()
    });
    store.post_message(Some(&p.id), "system", "all", "Kiểm tra tự động trước khi bàn giao: mở bản xem thử trên máy tính và điện thoại…", "system");
    let found = crate::qa::smoke(store, p).await;
    if blocked.is_empty() && found.is_empty() {
        store.post_message(Some(&p.id), "system", "all", "Kiểm tra tự động: không thấy lỗi (tệp 404, lỗi JavaScript, nút không nhìn thấy, tràn ngang).", "system");
        return Ok(false);
    }
    let mut lines: Vec<String> = blocked.iter().map(|(_, t)| format!("- Ticket bị chặn {t}")).collect();
    lines.extend(found.iter().map(|f| format!("- {f}")));
    let list = lines.join("\n");
    let round = store.project(&p.id).map(|x| x.gate_rounds).unwrap_or(0);
    if round < MAX_GATE_ROUNDS {
        store.update_project(&p.id, |p| p.gate_rounds += 1);
        // Blocked tickets get another go (the Tech Lead can split them); found problems become one bug ticket.
        for (id, _) in &blocked {
            store.update_ticket(id, |t| {
                t.status = TicketStatus::Todo;
                t.bounces = 0;
                t.notes.push("Mở lại trước khi bàn giao: sản phẩm chưa được bàn giao khi ticket này còn bị chặn. Làm theo cách khác / chia nhỏ nếu cần.".into());
            });
        }
        if !found.is_empty() {
            let seq = store.update_project(&p.id, |p| {
                p.ticket_seq += 1;
                p.ticket_seq
            });
            store.insert_ticket(Ticket {
                id: crate::store::new_id(),
                project_id: p.id.clone(),
                key: format!("T-{}", seq.unwrap_or(0)),
                title: "[Kiểm tra tự động] Lỗi người dùng gặp trên bản xem thử".into(),
                description: format!(
                    "Trình duyệt thật mở bản xem thử {} (1280px và 375px) và gặp:\n{}\n\nLưu ý: sản phẩm chạy dưới đường dẫn con /preview/<mã dự án>/ — mọi tài nguyên (âm thanh, ảnh, icon, worker) phải dùng đường dẫn tương đối hoặc import.meta.env.BASE_URL, không bắt đầu bằng \"/\".",
                    crate::config::preview_url(&p.id),
                    found.iter().map(|f| format!("- {f}")).collect::<Vec<_>>().join("\n")
                ),
                acceptance: vec![
                    "Mở bản xem thử ở 1280px và 375px: không còn tệp 404 và không còn lỗi JavaScript trong console".into(),
                    "Mọi nút có chữ đều nhìn thấy rõ (tương phản chữ / nền đạt WCAG AA)".into(),
                    "Không trang nào bị tràn ngang ở 375px".into(),
                ],
                kind: "bug".into(),
                status: TicketStatus::Todo,
                assignee: None,
                created_by: "system".into(),
                bounces: 0,
                notes: vec![],
                priority: 1,
                created_at: crate::store::now_ms(),
                updated_at: crate::store::now_ms(),
            });
        }
        store.post_message(Some(&p.id), "system", "all", format!("Chưa bàn giao được, đội sửa tiếp (vòng {}/{MAX_GATE_ROUNDS}):\n{list}", round + 1), "system");
        return Ok(true);
    }
    // Still not clean after the rounds allowed: the chairman decides instead of a silent "done".
    let a = store
        .request_approval(
            Some(&p.id),
            "system",
            ApprovalKind::Question,
            format!("Sản phẩm \"{}\" còn lỗi trước khi bàn giao", p.name),
            format!("Đội đã sửa {MAX_GATE_ROUNDS} vòng nhưng vẫn còn:\n{list}\n\nGhi \"bàn giao\" để bàn giao như hiện tại (lỗi được ghi trong báo cáo), hoặc ghi \"sửa tiếp\" / góp ý để đội làm thêm."),
            cancel,
        )
        .await?;
    let ans = a.answer.unwrap_or_default().trim().to_lowercase();
    let ship = a.status != ApprovalStatus::Answered || ["bàn giao", "ban giao", "ok", "duyệt", "được"].iter().any(|w| ans.starts_with(w));
    if ship {
        store.post_message(Some(&p.id), "system", "all", format!("Bàn giao với các lỗi còn lại (ghi vào báo cáo):\n{list}"), "system");
        return Ok(false);
    }
    store.update_project(&p.id, |p| p.gate_rounds = 0);
    if !ans.starts_with("sửa tiếp") && !ans.is_empty() {
        store.post_message(Some(&p.id), "chairman", "all", format!("Góp ý trước khi bàn giao: {ans}"), "chat");
    }
    Ok(true)
}

/// The PM writes the product specification and user guide, with real screenshots and flow diagrams.
async fn write_specification(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let Some(pm) = first(store, &[Role::Pm, Role::Ba, Role::Ceo]) else { return Ok(()) };
    if let Some(from) = first(store, &[Role::Tester, Role::Devops]) {
        handoff(store, &p.id, &from.id, &pm.id, "Sản phẩm đã qua kiểm định, nhờ PM viết tài liệu đặc tả và hướng dẫn sử dụng.");
    }
    let task = format!(
        "Sản phẩm đã hoàn thành và qua kiểm định. Viết TÀI LIỆU ĐẶC TẢ VÀ HƯỚNG DẪN SỬ DỤNG chuyên nghiệp (như PM của công ty phần mềm lớn bàn giao cho khách) tại docs/SPECIFICATION.md, đọc docs/ (PLAN, DECISIONS, REQUIREMENTS, DESIGN, ARCHITECTURE, DATA, QA-REPORT) và dùng thử sản phẩm thật ở bản xem thử {}.\nNội dung:\n1. Tổng quan: sản phẩm là gì, giải quyết vấn đề gì, dành cho ai; phiên bản; ngày bàn giao.\n2. Bảng danh sách tính năng (mã, tên, mô tả, trang / module).\n3. Từng màn hình: mục đích, các thành phần, thao tác, trạng thái (rỗng / lỗi / đang tải), quy tắc kiểm tra dữ liệu và thông báo lỗi — KÈM ẢNH CHỤP THẬT ở máy tính và điện thoại: dùng công cụ trình duyệt mở từng màn, browser_take_screenshot với filename \"docs/images/ten-man-hinh-desktop.png\" và \"docs/images/ten-man-hinh-mobile.png\" (browser_resize 375x812 cho điện thoại; tên tệp không dấu, kebab-case), rồi chèn vào tài liệu bằng ![mô tả](images/ten-man-hinh-desktop.png).\n4. Luồng người dùng chính: sơ đồ Mermaid (```mermaid flowchart```) cho từng luồng + mô tả từng bước.\n5. Quy tắc nghiệp vụ, dữ liệu được lưu (đơn giản hóa từ docs/DATA.md), quyền riêng tư.\n6. Hướng dẫn cài đặt / chạy / triển khai cho bên kỹ thuật của khách.\n7. Câu hỏi thường gặp, giới hạn hiện tại, đề xuất phát triển tiếp, lịch sử thay đổi.\nViết tiếng Việt, rõ ràng cho người không rành kỹ thuật, có mục lục. Nếu không có công cụ trình duyệt thì mô tả màn hình bằng lời. Xong gọi finish.{}",
        crate::config::preview_url(&p.id),
        revision_note(p)
    );
    agent::run(store, Job { employee_id: pm.id.clone(), project_id: p.id.clone(), phase: "documentation", ticket_id: None, case_id: None, task }, cancel).await?;
    store.post_message(Some(&p.id), &pm.id, "all", "Đã có tài liệu đặc tả và hướng dẫn sử dụng: docs/SPECIFICATION.md", "chat");
    Ok(())
}

/// The Security Engineer audits the code and dependencies; serious findings block the release.
async fn security_review(store: &S, p: &Project, cancel: &CancellationToken) -> Result<bool, String> {
    let Some(sec) = first(store, &[Role::Security]) else { return Ok(false) };
    store.update_project(&p.id, |p| p.security_reviews += 1);
    if let Some(from) = first(store, &[Role::Tester, Role::Dev]) {
        handoff(store, &p.id, &from.id, &sec.id, "Code đã xong, nhờ Security kiểm tra bảo mật trước khi phát hành.");
    }
    let task = format!(
        "Kiểm tra BẢO MẬT toàn bộ dự án trước khi phát hành, theo kỹ năng Security của bạn:\n1. Thư viện: chạy `npm audit --omit=dev` (hoặc công cụ tương đương của ngôn ngữ) nếu có tệp khai báo phụ thuộc.\n2. Bí mật: tìm khóa API, mật khẩu, token, chuỗi kết nối bị ghi cứng trong code / cấu hình / lịch sử (`git log -p` nếu cần).\n3. Code theo OWASP Top 10: XSS (innerHTML, chèn HTML chưa lọc), injection, xác thực / phân quyền, kiểm tra dữ liệu đầu vào, lưu dữ liệu nhạy cảm, CORS / CSP / header bảo mật, đường dẫn tệp, tải tệp lên, lộ thông tin lỗi.\n4. Với giao diện web: mở bản xem thử {} bằng trình duyệt nếu có công cụ, thử nhập chuỗi tấn công XSS vào các ô nhập.\nMỗi lỗ hổng mức Cao / Nghiêm trọng gọi report_issue (mô tả, vị trí, cách khai thác, cách sửa cụ thể). Lỗi mức Thấp / Trung bình chỉ ghi vào báo cáo. Viết docs/SECURITY.md: phạm vi đã kiểm, kết quả từng hạng mục, danh sách phát hiện theo mức độ, khuyến nghị. Xong gọi finish với kết luận có an toàn để phát hành không.{}",
        crate::config::preview_url(&p.id),
        revision_note(p)
    );
    let r = agent::run(store, Job { employee_id: sec.id.clone(), project_id: p.id.clone(), phase: "security", ticket_id: None, case_id: None, task }, cancel).await?;
    if r.created_tickets > 0 {
        if let Some(dev) = first(store, &[Role::Dev]) {
            handoff(store, &p.id, &sec.id, &dev.id, format!("Có {} lỗ hổng bảo mật cần sửa trước khi phát hành.", r.created_tickets));
        }
        return Ok(true);
    }
    store.post_message(Some(&p.id), &sec.id, "all", format!("Kiểm tra bảo mật: {}", summary_of(&r.outcome)), "chat");
    Ok(false)
}

/// After hand-over the team looks back; lessons go to docs/RETRO.md and the company memory.
async fn retrospective(store: &S, p: &Project, cancel: &CancellationToken) {
    let Some(host) = first(store, &[Role::Pm, Role::Ceo]) else { return };
    let tickets = store.tickets_of(&p.id);
    let mut stats = String::new();
    for t in &tickets {
        stats.push_str(&format!("- {} [{:?}] trả lại {} lần: {}\n", t.key, t.status, t.bounces, t.title));
    }
    let task = format!(
        "Họp RÚT KINH NGHIỆM (retrospective) sau khi bàn giao dự án \"{}\". Số liệu ticket:\n{stats}\nĐọc docs/QA-REPORT.md, docs/SECURITY.md, docs/DESIGN-REVIEW.md, docs/RELEASE.md, docs/DECISIONS.md (nếu có). Viết docs/RETRO.md: điều làm tốt; điều chưa tốt và NGUYÊN NHÂN GỐC (vì sao ticket bị trả lại, lỗi nào lọt tới kiểm định cuối, bước nào tốn công nhất); bài học; 3–5 hành động cụ thể cho dự án sau. Nếu có công cụ memory__, lưu 3–5 bài học quan trọng nhất vào bộ nhớ chung của công ty (ngắn gọn, dùng được cho dự án khác). Xong gọi finish với tóm tắt.",
        p.name
    );
    match agent::run(store, Job { employee_id: host.id.clone(), project_id: p.id.clone(), phase: "retro", ticket_id: None, case_id: None, task }, cancel).await {
        Ok(r) => {
            store.post_message(Some(&p.id), &host.id, "all", format!("Rút kinh nghiệm: {}", summary_of(&r.outcome)), "chat");
        }
        Err(e) if e != ABORTED => store.log_activity(&host.id, Some(&p.id), "error", format!("Không họp rút kinh nghiệm được: {e}")),
        _ => {}
    };
}

/// Before hand-over the Tester re-tests the WHOLE product (regression + every acceptance criterion).
async fn final_qa(store: &S, p: &Project, cancel: &CancellationToken) -> Result<bool, String> {
    let Some(tester) = first(store, &[Role::Tester]) else { return Ok(false) };
    store.update_project(&p.id, |p| p.qa_rounds += 1);
    if let Some(from) = first(store, &[Role::Devops]) {
        handoff(store, &p.id, &from.id, &tester.id, "Bản phát hành đã build xong, nhờ QA kiểm định toàn bộ lần cuối.");
    }
    let task = if is_standard(p) {
        format!(
            "KIỂM ĐỊNH CUỐI trước khi bàn giao Chủ tịch (dự án cỡ vừa: MỘT lần kiểm định gộp cả soát giao diện).\n1. Chạy lint / typecheck / test / build của dự án (có bước build thì build để bản xem thử chạy được).\n2. Mở bản xem thử {} bằng trình duyệt thật ở 1280px và 375px, đi qua TỪNG tiêu chí nghiệm thu trong docs/REQUIREMENTS.md như người dùng mới: bấm mọi nút / liên kết, tải lại trang, xem console và tệp 404.\n3. Soát giao diện so với design/mockup.html (nếu có): lệch, tràn ngang, chữ bị cắt / khó đọc, tương phản kém, sai màu / font.\nMỗi lỗi THẬT gọi report_issue (bước tái hiện, mong đợi / thực tế, cỡ màn hình) — hệ thống chỉ biết có lỗi qua report_issue; gộp lỗi nhỏ cùng trang vào một; không báo lỗi vì sở thích cá nhân. Viết docs/QA-REPORT.md ngắn: đã kiểm gì, kết quả, lỗi còn lại, kết luận. Đảm bảo README.md có cách cài / chạy. Xong gọi finish với kết luận có đủ chất lượng bàn giao không.{}",
            crate::config::preview_url(&p.id),
            revision_note(p)
        )
    } else { format!(
        "KIỂM ĐỊNH CUỐI trước khi bàn giao Chủ tịch — tiêu chuẩn sản phẩm thương mại, không phải bài tập. Theo kỹ năng Tester của bạn:
1. Chạy toàn bộ lint / typecheck / test / build của dự án.
2. Chạy lại TOÀN BỘ test case trong docs/qa/TEST-CASES.md (regression) và mọi tiêu chí nghiệm thu trong docs/REQUIREMENTS.md trên trình duyệt thật ở bản xem thử {} — cả 1280px và 375px — như một người dùng mới: luồng chính, ca biên, dữ liệu rỗng / nhiều, tải lại trang, lỗi console.
3. Soát chất lượng tổng thể: giao diện có chỗ nào xấu / lệch / khó hiểu, chữ tiếng Việt, thông báo lỗi, tốc độ.
Bấm thử TỪNG nút / liên kết chính trên trang thật (nút phải nhìn thấy rõ và bấm được), mở thẳng từng đường dẫn con, xem console và tệp tải lỗi (404). Nếu dự án có test giao diện (Playwright / e2e) thì chạy chúng trên bản xem thử thật (vd. PLAYWRIGHT_TEST_BASE_URL=<địa chỉ bản xem thử>), không chỉ trên máy chủ dev.
Mỗi lỗi THẬT BẮT BUỘC gọi report_issue (bước tái hiện, mong đợi / thực tế, cỡ màn hình, mức độ) — hệ thống CHỈ biết có lỗi qua report_issue; ghi vào tệp hay nhắn tin thì lỗi vẫn bị coi là không có — và ghi vào docs/qa/BUGS.md. Chỉ kết luận \"đủ điều kiện bàn giao\" khi không còn lỗi và không còn ticket bị chặn. Cập nhật trạng thái mới nhất trong TEST-CASES.md và RTM.md; xuất thêm docs/qa/test-cases.csv và docs/qa/bugs.csv (mở được bằng Excel / Google Sheets). Viết docs/QA-REPORT.md dạng Test Summary Report: tổng số case, Pass / Fail / Blocked / Not Run và % đạt, số bug theo Severity và trạng thái, độ phủ RTM (FR có case đạt / tổng FR), exit criteria trong TEST-PLAN đạt chưa, rủi ro còn lại, kết luận đủ điều kiện phát hành hay không. Xong gọi finish với kết luận có đủ chất lượng bàn giao không.{}",
        crate::config::preview_url(&p.id),
        revision_note(p)
    ) };
    let r = agent::run(store, Job { employee_id: tester.id.clone(), project_id: p.id.clone(), phase: "final_qa", ticket_id: None, case_id: None, task }, cancel).await?;
    if r.created_tickets > 0 {
        if let Some(dev) = first(store, &[Role::Dev]) {
            handoff(store, &p.id, &tester.id, &dev.id, format!("Kiểm định cuối còn {} lỗi, nhờ Dev sửa.", r.created_tickets));
        }
        return Ok(true);
    }
    store.post_message(Some(&p.id), &tester.id, "all", format!("Kiểm định cuối: {}", summary_of(&r.outcome)), "chat");
    Ok(false)
}

/// The Designer looks at the real product in a browser and files tickets for visual problems.
async fn design_review(store: &S, p: &Project, cancel: &CancellationToken) -> Result<bool, String> {
    let Some(designer) = first(store, &[Role::Designer]) else { return Ok(false) };
    store.update_project(&p.id, |p| p.design_reviews += 1);
    if let Some(from) = first(store, &[Role::Tester, Role::Dev]) {
        handoff(store, &p.id, &from.id, &designer.id, "Các ticket đã xong, nhờ Designer duyệt giao diện thật.");
    }
    let task = format!(
        "Duyệt giao diện sản phẩm THẬT so với docs/DESIGN.md, design/tokens.css và design/mockup.html, theo kỹ năng Designer của bạn. Mở bản xem thử {} bằng công cụ trình duyệt, đi qua từng trang và trạng thái chính, chụp màn hình ở 1280px và 375px. Với mỗi lỗi giao diện thật (lệch, chồng chéo, tràn ngang, chữ bị cắt, tương phản kém, sai token, thiếu trạng thái, không nhất quán, trông thiếu chuyên nghiệp) gọi create_ticket: tiêu chí ghi rõ vị trí, hiện trạng, cách sửa cụ thể (token / CSS). Gộp lỗi nhỏ cùng trang vào một ticket. Ghi nhận xét vào docs/DESIGN-REVIEW.md. Nếu không có công cụ trình duyệt thì đọc CSS / HTML để soát. Không tạo ticket vì sở thích cá nhân. Xong gọi finish.{}",
        crate::config::preview_url(&p.id),
        revision_note(p)
    );
    let r = agent::run(store, Job { employee_id: designer.id.clone(), project_id: p.id.clone(), phase: "design_review", ticket_id: None, case_id: None, task }, cancel).await?;
    if r.created_tickets > 0 {
        if let Some(dev) = first(store, &[Role::Dev]) {
            handoff(store, &p.id, &designer.id, &dev.id, format!("Giao diện còn {} điểm cần sửa, nhờ Dev xử lý.", r.created_tickets));
        }
        return Ok(true);
    }
    store.post_message(Some(&p.id), &designer.id, "all", format!("Duyệt giao diện: {}", summary_of(&r.outcome)), "chat");
    Ok(false)
}

pub(crate) async fn release_check(store: &S, p: &Project, allow_bugs: bool, cancel: &CancellationToken) -> Result<bool, String> {
    let Some(ops) = first(store, &[Role::Devops]) else { return Ok(false) };
    if let Some(from) = first(store, &[Role::Tester, Role::Dev]) {
        handoff(store, &p.id, &from.id, &ops.id, "Các ticket đã xong, nhờ DevOps kiểm tra bản phát hành.");
    }
    let task = format!(
        "Kiểm tra sản phẩm có chạy được không: xem các tệp, chạy lệnh build/test phù hợp (vd. node --check, npm install && npm run build, python -m py_compile, cargo build). Đối chiếu cấu trúc dự án với docs/ARCHITECTURE.md và chuẩn công ty (README, CHANGELOG, .gitignore, .editorconfig, src/app, src/pages, src/modules, src/shared đúng quy tắc phụ thuộc, tests/, CI, không có tệp trùng lặp). Nếu dự án có bước build thì chạy build ra dist/ (đường dẫn tương đối) để Chủ tịch mở xem thử được. Viết hoặc cập nhật README.md ở gốc dự án: giới thiệu, yêu cầu, cách cài, cách chạy, cách test, cấu trúc thư mục. Cập nhật CHANGELOG.md. Ghi docs/RELEASE.md: đã kiểm tra gì, kết quả.{} Xong gọi finish.{}",
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
    let all = members(store, p);
    meeting(store, &p.id, all.clone(), "Họp tổng kết", true);
    let task = format!(
        "Đội đã xử lý các ticket:\n{list}\nĐọc docs/ (PLAN, REQUIREMENTS, SPRINT, DESIGN, RELEASE nếu có) và xem các tệp sản phẩm. Viết docs/REPORT.md: đã làm gì (nhắc tài liệu đặc tả docs/SPECIFICATION.md cho khách), cách chạy/mở sản phẩm, hạn chế còn lại (nêu rõ ticket bị Blocked nếu có), ghi nhận thành viên nổi bật. Sau đó gọi finish với lời báo cáo ngắn gửi Chủ tịch.{}",
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
    let minutes_result = if is_small(p) { Ok(()) } else { minutes(store, p, "Họp tổng kết", "docs/REPORT.md", cancel).await };
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
        team.extend(team_staff(store, *role).into_iter().map(|e| e.id));
    }
    team.extend(team_staff(store, Role::Secretary).into_iter().map(|e| e.id));
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

#[cfg(test)]
mod design_choice_tests {
    use super::*;

    fn opts() -> Vec<ApprovalOption> {
        ["a", "b", "c"].iter().map(|id| ApprovalOption { id: id.to_string(), title: format!("T{id}"), note: String::new(), images: vec![], link: String::new() }).collect()
    }

    #[test]
    fn strips_repeated_option_letter() {
        assert_eq!(strip_option_letter("A · Gambit Bản thảo", "a"), "Gambit Bản thảo");
        assert_eq!(strip_option_letter("Phương án B: Đấu Trường", "b"), "Đấu Trường");
        assert_eq!(strip_option_letter("Aurora", "a"), "Aurora");
        assert_eq!(strip_option_letter("Gambit", "a"), "Gambit");
    }

    #[test]
    fn reads_choice_and_feedback() {
        let o = opts();
        assert_eq!(parse_choice("Chọn phương án B — Tb", &o), (Some(1), None));
        assert_eq!(parse_choice("Chọn phương án C — Tc\nGóp ý: chữ to hơn", &o), (Some(2), Some("chữ to hơn".into())));
        assert_eq!(parse_choice("b", &o), (Some(1), None));
        assert_eq!(parse_choice("B: thêm màu xanh", &o), (Some(1), Some("thêm màu xanh".into())));
        assert_eq!(parse_choice("phương án a nhưng nền tối hơn\nbo góc ít thôi", &o).0, Some(0));
        // Plain feedback without a letter: no pick, the whole text is feedback.
        assert_eq!(parse_choice("màu ấm hơn", &o), (None, Some("màu ấm hơn".into())));
        assert_eq!(parse_choice("", &o), (None, None));
    }

    #[test]
    fn finds_options_with_titles_and_screenshots() {
        let dir = std::env::temp_dir().join(format!("aico-opts-{}", crate::store::new_id()));
        for id in ["a", "c"] {
            let d = dir.join("design/options").join(id);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("mockup.html"), "<html></html>").unwrap();
        }
        std::fs::write(dir.join("design/options/a/README.md"), "# Tối giản\n\nNhiều khoảng trắng.").unwrap();
        std::fs::write(dir.join("design/options/a/desktop.png"), b"png").unwrap();
        let o = design_options(&dir);
        assert_eq!(o.len(), 2);
        assert_eq!((o[0].title.as_str(), o[0].note.as_str()), ("Tối giản", "Nhiều khoảng trắng."));
        assert_eq!(o[0].images, vec!["design/options/a/desktop.png".to_string()]);
        assert_eq!((o[1].id.as_str(), o[1].title.as_str(), o[1].link.as_str()), ("c", "Phương án C", "design/options/c/mockup.html"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod vote_tests {
    use super::*;

    fn emp(id: &str, role: Role) -> Employee {
        let mut e = crate::config::default_employees().remove(0);
        e.id = id.into();
        e.name = id.into();
        e.role = role;
        e
    }

    #[test]
    fn ballot_votes_and_tally() {
        let ballot = "# Phiếu\n\n## V1: Dùng framework nào?\n- A: HTML thuần (đề xuất bởi Dev)\n- B: React (đề xuất bởi Architect)\n\n## V2: Lưu dữ liệu\n- A: localStorage\n- B: Server\n- C: IndexedDB\n\n## V3: chỉ một phương án\n- A: x\n";
        let issues = ballot_issues(ballot);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[1].2.len(), 3);
        let v = |t: &str| parse_votes(t);
        assert_eq!(v("V1: B — vì dễ mở rộng\n**V2**: c lý do"), vec![("V1".to_string(), 'B'), ("V2".to_string(), 'C')]);
        let ballots = vec![
            (emp("ceo", Role::Ceo), v("V1: A\nV2: A")),
            (emp("dev", Role::Dev), v("V1: A\nV2: B")),
            (emp("arch", Role::Architect), v("- V1: B\n- V2: B")),
            (emp("ba", Role::Ba), v("V1: B\nV2: A")),
            (emp("qa", Role::Tester), v("V1: B\nV9: A")),
        ];
        let md = tally(&issues, &ballots, "ceo");
        assert!(md.contains("**V1 → B: 3 phiếu**"), "{md}");
        // V2: A=2 (ceo, ba), B=2 (dev, arch) → tie, CEO's pick wins.
        assert!(md.contains("**V2 → A: 2 phiếu (hòa phiếu, phiếu CEO quyết)**"), "{md}");
    }

    #[test]
    fn resume_prioritizes_testing_then_review_then_todo() {
        let mk = |key: &str, status: TicketStatus, prio: u8, ts: u64| Ticket {
            id: key.into(),
            project_id: "p".into(),
            key: key.into(),
            title: key.into(),
            description: String::new(),
            kind: "feature".into(),
            status,
            priority: prio,
            assignee: None,
            acceptance: vec![],
            notes: vec![],
            bounces: 0,
            created_by: "pm".into(),
            created_at: ts,
            updated_at: ts,
        };
        let mut tks = vec![
            mk("T-6", TicketStatus::Todo, 1, 1),
            mk("T-4", TicketStatus::Review, 2, 2),
            mk("T-5", TicketStatus::Testing, 3, 3),
        ];
        tks.sort_by_key(|t| ticket_pick_key(t));
        let keys: Vec<&str> = tks.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(keys, vec!["T-5", "T-4", "T-6"]);
    }
}

#[cfg(test)]
mod change_scope_tests {
    use super::change_scope_of;

    #[test]
    fn reads_the_scope_line() {
        assert_eq!(change_scope_of("PHẠM VI: thiet-ke, code\nĐã hủy T-4…"), "design,code");
        assert_eq!(change_scope_of("Tóm tắt\nPhạm vi: kiến trúc"), "arch,code");
        assert_eq!(change_scope_of("PHẠM VI: code"), "code");
        // No scope line: only the code is redone.
        assert_eq!(change_scope_of("Đã sắp lại backlog"), "code");
    }
}
