//! The company process. Phases run in order; a phase whose role is not on staff is skipped.
//!
//! Plan (CEO) → Requirements (BA) → Planning (PM) → Design (Designer) → Architecture (Tech Lead)
//! → Build (Dev ⇄ Reviewer ⇄ Tester, PM standup) → Release (DevOps; bugs go back to Build)
//! → Report (CEO) → Chairman approval. The Secretary writes minutes of the kickoff and closing meetings.
//! At kickoff the department heads discuss the CEO's draft for `settings.meeting_rounds` rounds before the CEO decides.
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

/// Kickoff: the CEO drafts a plan, the department heads discuss it over several rounds
/// (opinions, then rebuttals), and only then the CEO decides and finalises the plan.
async fn phase_plan(store: &S, p: &Project, cancel: &CancellationToken) -> Result<(), String> {
    let ceo = first(store, &[Role::Ceo]).ok_or("Công ty cần ít nhất 1 CEO đang làm việc")?;
    let all = everyone(store);
    store.post_message(Some(&p.id), "chairman", &ceo.id, if p.feedback.is_empty() { p.request.clone() } else { p.feedback.last().cloned().unwrap_or_default() }, "chat");
    meeting(store, &p.id, all.clone(), "Họp khởi động", true);
    let r = match kickoff(store, p, &ceo, &all, cancel).await {
        Ok(r) => r,
        Err(e) => {
            meeting(store, &p.id, all, "", false);
            return Err(e);
        }
    };
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
    const ROLES: [Role; 12] = [
        Role::Ba,
        Role::Pm,
        Role::Architect,
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
    ROLES.iter().filter_map(|r| store.staff(*r).into_iter().next()).collect()
}

async fn kickoff(store: &S, p: &Project, ceo: &Employee, all: &[String], cancel: &CancellationToken) -> Result<agent::JobResult, String> {
    let rounds = store.with(|s| s.settings.meeting_rounds).min(5);
    let panel = if rounds == 0 { vec![] } else { discussion_panel(store) };
    let dir = if p.feedback.is_empty() { "docs/meetings/kickoff".to_string() } else { format!("docs/meetings/kickoff-sua-{}", p.feedback.len()) };
    // Resumable: whatever is already written to disk (opinions, ballot, votes) is not redone after a restart.
    let pdir = store.workspace_dir.join(&p.slug);
    let has = |rel: &str| std::fs::metadata(pdir.join(rel)).map(|m| m.len() > 0).unwrap_or(false);
    let started = !panel.is_empty()
        && std::fs::read_dir(pdir.join(&dir)).map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with("v1-"))).unwrap_or(false);

    // 1. The CEO drafts.
    let draft_note = if panel.is_empty() {
        ""
    } else {
        " Đây là BẢN NHÁP để đưa ra cuộc họp: các trưởng bộ phận sẽ góp ý và phản biện trước khi bạn chốt, nên hãy nêu rõ các phương án bạn còn phân vân."
    };
    let task = format!(
        "Chủ tịch giao dự án \"{}\":\n{}{}\n\nViệc của bạn: phân tích yêu cầu và viết kế hoạch ngắn gọn vào docs/PLAN.md (mục tiêu, phạm vi, công nghệ gợi ý — ưu tiên thứ đơn giản chạy ngay, các hạng mục chính, ai trong công ty làm gì).{draft_note} Chỉ hỏi Chủ tịch (ask_chairman) khi thật sự không thể tự quyết. Xong gọi finish.",
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
                    "Cuộc họp khởi động dự án \"{}\". Yêu cầu của Chủ tịch:\n{}\n\nCEO đã đưa bản nháp kế hoạch ở docs/PLAN.md. Với góc nhìn chuyên môn của {role}, đọc kế hoạch rồi viết ý kiến vào {file}:\n1. Điểm bạn đồng ý.\n2. Rủi ro / vấn đề bạn thấy (cụ thể, có lý do).\n3. Đề xuất để tối ưu: đơn giản hơn, nhanh hơn, rẻ hơn hoặc chất lượng hơn.\n4. Phương án thay thế nếu bạn không đồng ý với hướng đi.\n5. Phần việc của bạn: cần gì, ước lượng công sức.\nThẳng thắn, có lập luận — không khen xã giao, không chép lại kế hoạch. Ngắn gọn, khoảng một trang, gạch đầu dòng, viết trọn câu — ghi một lần là xong, không cần đếm chữ hay sửa đi sửa lại. Chỉ ghi đúng tệp này, không sửa tệp khác. Xong gọi finish với tóm tắt 1–2 câu ý kiến chính của bạn.",
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
        "Bạn chủ trì cuộc họp khởi động dự án \"{}\". Đọc docs/PLAN.md và toàn bộ ý kiến, phản biện trong {dir}/. Tổng hợp các vấn đề còn tranh cãi hoặc có nhiều cách làm (tối đa 6 vấn đề quan trọng nhất) thành phiếu bầu, ghi vào {ballot_file} ĐÚNG định dạng sau:\n\n## V1: <vấn đề cần quyết>\n- A: <phương án> (đề xuất bởi <vai trò>)\n- B: <phương án> (đề xuất bởi <vai trò>)\n\n## V2: ...\n\nMỗi vấn đề 2–4 phương án, trình bày công bằng, ghi kèm ưu/nhược ngắn gọn của từng phương án ngay trên dòng đó. Không tự quyết ở bước này. Nếu mọi người đã đồng thuận hết thì ghi tệp chỉ với dòng \"Không có vấn đề cần bỏ phiếu\". Xong gọi finish.",
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
}
