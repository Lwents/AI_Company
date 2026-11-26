//! Agent Harness memory layer (inspired by "Effective Harnesses for Long-Running Agents"):
//! 1. Rolling project progress ledger (`docs/PROGRESS.md`) + live Kanban & Git orientation briefing.
//! 2. Automatic handoff checkpoint when an employee runs out of steps (persisted to `docs/PROGRESS.md` and ticket notes).
//! 3. Intelligent context compaction using `light_model` while strictly preserving User/Assistant alternation and tool_call pairs.
use crate::config;
use crate::llm::{self, LlmMessage, LlmRequest, Meta};
use crate::store::{now_ms, S};
use crate::types::{Employee, Project, ProviderId, TicketStatus};
use std::path::Path;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const PROGRESS_REL: &str = "docs/PROGRESS.md";
const MAX_PROGRESS_ENTRIES: usize = 16;
const MAX_BRIEFING_PROGRESS_CHARS: usize = 3500;
const MAX_GIT_BRIEF_CHARS: usize = 1200;
const HEADER: &str = "# Sổ tiến độ dự án (Agent Harness Memory)\n\n> Tệp này được hệ thống tự cập nhật sau mỗi mốc ticket và mỗi lần bàn giao dở dang để nhân viên tiếp theo nắm ngữ cảnh ngay mà không phải đọc lại toàn bộ dự án.\n";
const ENTRY_SEP: &str = "\n---\n";

/// Phases where injecting the project Harness briefing helps orient the agent quickly.
pub fn wants_briefing(phase: &str) -> bool {
    matches!(
        phase,
        "implement" | "review" | "test" | "release" | "final_qa" | "design_review" | "security" | "documentation" | "report" | "architecture"
    )
}

/// Formats UTC+7 HH:MM timestamp for progress entries (no chrono dependency needed).
fn hhmm_vn(ms: u64) -> String {
    let secs = (ms / 1000).saturating_add(7 * 3600);
    let day_secs = secs % 86_400;
    format!("{:02}:{:02}", day_secs / 3600, (day_secs % 3600) / 60)
}

/// Appends one structured progress/checkpoint entry to `<dir>/docs/PROGRESS.md`, keeping the latest entries bounded.
pub fn record_entry(dir: &Path, heading: &str, body: &str) {
    let body = body.trim();
    if body.is_empty() {
        return;
    }
    let path = dir.join(PROGRESS_REL);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut entries: Vec<String> = existing
        .split(ENTRY_SEP)
        .skip(1) // skip header block
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let ts = hhmm_vn(now_ms());
    let clamped_body: String = body.chars().take(1800).collect();
    entries.push(format!("### [{ts}] {heading}\n{clamped_body}"));
    if entries.len() > MAX_PROGRESS_ENTRIES {
        let drop_n = entries.len() - MAX_PROGRESS_ENTRIES;
        entries.drain(..drop_n);
    }
    let mut out = String::from(HEADER);
    for e in &entries {
        out.push_str(ENTRY_SEP);
        out.push_str(e);
        out.push('\n');
    }
    let _ = std::fs::write(&path, out);
}

async fn git_quick(dir: &Path, args: &[&str]) -> Option<String> {
    if !dir.join(".git").exists() {
        return None;
    }
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(args).current_dir(dir).env("GIT_TERMINAL_PROMPT", "0").kill_on_drop(true);
    if let Some(parent) = dir.parent() {
        cmd.env("GIT_CEILING_DIRECTORIES", parent);
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let out = tokio::time::timeout(Duration::from_secs(6), cmd.output()).await.ok()?.ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Builds a concise orientation snapshot (Kanban status + recent Git state + latest `docs/PROGRESS.md` entries).
pub async fn briefing(store: &S, project: &Project, dir: &Path) -> String {
    let mut sections: Vec<String> = vec![];

    // 1. Live Kanban summary.
    let tickets = store.tickets_of(&project.id);
    if !tickets.is_empty() {
        let mut done = 0usize;
        let mut blocked = 0usize;
        let mut open = vec![];
        for t in &tickets {
            match t.status {
                TicketStatus::Done => done += 1,
                TicketStatus::Blocked => {
                    blocked += 1;
                    open.push(format!("{} [Blocked] {}", t.key, t.title));
                }
                st => open.push(format!("{} [{st:?}] {}", t.key, t.title)),
            }
        }
        let mut kb = format!("Kanban: {done}/{} ticket Done (Blocked: {blocked}).", tickets.len());
        if !open.is_empty() {
            kb.push_str("\nCòn mở: ");
            kb.push_str(&open.iter().take(8).cloned().collect::<Vec<_>>().join(" | "));
        }
        sections.push(kb);
    }

    // 2. Live Git snapshot (recent commits + dirty working tree).
    let (log_out, status_out) = tokio::join!(
        git_quick(dir, &["log", "-n", "4", "--oneline"]),
        git_quick(dir, &["status", "-s"])
    );
    let mut git_lines = vec![];
    if let Some(l) = log_out {
        git_lines.push(format!("Commit gần nhất:\n{l}"));
    }
    if let Some(st) = status_out {
        let preview: String = st.chars().take(MAX_GIT_BRIEF_CHARS).collect();
        git_lines.push(format!("Tệp đang thay đổi chưa commit (git status -s):\n{preview}"));
    }
    if !git_lines.is_empty() {
        sections.push(git_lines.join("\n"));
    }

    // 3. Latest progress ledger entries from `docs/PROGRESS.md`.
    if let Ok(raw) = std::fs::read_to_string(dir.join(PROGRESS_REL)) {
        let entries: Vec<&str> = raw.split(ENTRY_SEP).skip(1).map(str::trim).filter(|s| !s.is_empty()).collect();
        if !entries.is_empty() {
            let mut tail_entries: Vec<&str> = entries.into_iter().rev().take(5).collect();
            tail_entries.reverse();
            let joined = tail_entries.join("\n\n");
            let clipped: String = joined.chars().take(MAX_BRIEFING_PROGRESS_CHARS).collect();
            sections.push(format!("Nhật ký bàn giao gần nhất (docs/PROGRESS.md):\n{clipped}"));
        }
    }

    if sections.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n# BỘ NHỚ TIẾN ĐỘ DỰ ÁN (HARNESS — ĐỌC ĐẦU TIÊN, KHÔNG LÀM LẠI VIỆC ĐÃ XONG)\n{}\n",
            sections.join("\n\n")
        )
    }
}

/// Picks a lightweight employee clone (using `settings.light_model` when configured) for summary/compaction calls.
fn light_employee(store: &S, emp: &Employee) -> Employee {
    let mut e = emp.clone();
    let light = store.with(|s| s.settings.light_model.trim().to_string());
    if !light.is_empty() && e.provider != ProviderId::Mock {
        e.model = light;
    }
    e
}

/// Renders a slice of conversation messages into compact plain text for summarization.
fn render_for_summary(messages: &[LlmMessage], max_chars: usize) -> String {
    let mut out = String::new();
    for m in messages {
        match m {
            LlmMessage::Assistant { text, tool_calls, .. } => {
                if !text.trim().is_empty() {
                    let t: String = text.trim().chars().take(450).collect();
                    out.push_str(&format!("[Assistant] {t}\n"));
                }
                for c in tool_calls {
                    let arg_preview: String = c.input.to_string().chars().take(240).collect();
                    out.push_str(&format!("[ToolCall] {}({arg_preview})\n", c.name));
                }
            }
            LlmMessage::User { text, tool_results } => {
                for r in tool_results {
                    let head: String = r.content.trim().chars().take(500).collect();
                    let tag = if r.is_error { "ToolErr" } else { "ToolRes" };
                    out.push_str(&format!("[{tag}] {head}\n"));
                }
                if let Some(t) = text.as_deref().filter(|s| !s.trim().is_empty()) {
                    let head: String = t.trim().chars().take(350).collect();
                    out.push_str(&format!("[User] {head}\n"));
                }
            }
        }
        if out.chars().count() >= max_chars {
            break;
        }
    }
    out.chars().take(max_chars).collect()
}

/// Generates a handoff checkpoint when an employee runs out of steps, saving it to `docs/PROGRESS.md` and `ticket.notes`.
pub async fn save_handoff_checkpoint(
    store: &S,
    emp: &Employee,
    project: &Project,
    dir: &Path,
    ticket_id: Option<&str>,
    messages: &[LlmMessage],
    cancel: &CancellationToken,
) -> Option<String> {
    if emp.provider == ProviderId::Mock || cancel.is_cancelled() {
        return None;
    }
    let summarizer = light_employee(store, emp);
    let transcript = render_for_summary(messages.get(1..).unwrap_or(&[]), 18_000);
    if transcript.trim().is_empty() {
        return None;
    }
    let ticket_label = ticket_id
        .and_then(|id| store.ticket(id))
        .map(|t| format!("{} — {}", t.key, t.title))
        .unwrap_or_else(|| project.name.clone());
    let prompt = format!(
        "Nhân viên {} ({}) vừa chạm giới hạn bước khi làm \"{ticket_label}\".\n\
Hãy tổng hợp BẢN BÀN GIAO TIẾN ĐỘ (Checkpoint) cực kỳ súc tích (tối đa 12 gạch đầu dòng) từ nhật ký bên dưới để đồng nghiệp nối tiếp NGAY LẬP TỨC mà không phải đọc lại từ đầu:\n\
1. Các tệp ĐÃ tạo / sửa trên đĩa (ghi rõ đường dẫn và đã xong phần nào).\n\
2. Kết quả test gần nhất (bài test nào đã xanh, lỗi/timeout cụ thể nào còn lại kèm tên hàm/file).\n\
3. Kiến thức cốt lõi đã tìm thấy (tên hàm, schema, số dòng quan trọng) để người sau không cần đọc lại.\n\
4. Đúng 1–3 việc tiếp theo cần làm ngay để hoàn thành.\n\
Chỉ trả về nội dung bàn giao tiếng Việt, không chào hỏi.\n\n--- Nhật ký lượt làm việc ---\n{transcript}",
        emp.name,
        config::role_label(emp.role),
    );
    let meta = Meta {
        phase: "harness_checkpoint",
        project_id: project.id.clone(),
        extra: String::new(),
        project_name: project.name.clone(),
        request: String::new(),
        ticket: None,
    };
    let req_msgs = vec![LlmMessage::User { text: Some(prompt), tool_results: vec![] }];
    let res = llm::chat(
        store,
        LlmRequest {
            employee: &summarizer,
            system: "Bạn là hệ thống Harness tóm tắt bàn giao kỹ thuật chính xác, ngắn gọn, giữ nguyên tên tệp, tên hàm và lỗi thật.",
            messages: &req_msgs,
            tools: &[],
            max_tokens: 1800,
            meta: &meta,
        },
        cancel,
    )
    .await
    .ok()?;
    store.add_usage(&emp.id, Some(&project.id), res.input_tokens, res.output_tokens);
    let cp = res.text.trim().to_string();
    if cp.chars().count() < 20 {
        return None;
    }
    let heading = format!("Checkpoint bàn giao · {} ({}) · {}", emp.name, config::role_label(emp.role), ticket_label);
    record_entry(dir, &heading, &cp);
    if let Some(tid) = ticket_id {
        let short_cp: String = cp.chars().take(1200).collect();
        store.update_ticket(tid, |t| t.notes.push(format!("{} (Checkpoint bàn giao): {short_cp}", emp.name)));
    }
    store.log_activity(&emp.id, Some(&project.id), "result", format!("Đã lưu Harness Checkpoint vào {PROGRESS_REL}"));
    Some(cp)
}

/// Rebuilds `messages` replacing `messages[1..split]` with a single Working Memory summary while preserving:
/// - `messages[0]` (original user task)
/// - strict User/Assistant alternation (`User -> Assistant(ack) -> User(memory) -> Assistant(split) -> ...`)
/// - all recent `tool_call` / `tool_result` pairs intact from `split` onward.
pub fn apply_compaction(messages: &mut Vec<LlmMessage>, split: usize, summary: &str) -> bool {
    if split < 5 || split >= messages.len() || split % 2 == 0 {
        return false;
    }
    if !matches!(messages.first(), Some(LlmMessage::User { .. })) || !matches!(messages.get(split), Some(LlmMessage::Assistant { .. })) {
        return false;
    }
    let first = messages[0].clone();
    let tail = messages[split..].to_vec();
    let compacted_turns = (split - 1) / 2;
    let ack = LlmMessage::Assistant {
        text: "Đã ghi nhận Bộ nhớ làm việc (Harness) cho các bước trước, tiếp tục thực hiện ngay.".into(),
        tool_calls: vec![],
        raw: None,
        raw_provider: None,
    };
    let memory = LlmMessage::User {
        text: Some(format!(
            "[BỘ NHỚ LÀM VIỆC — HARNESS COMPACTION (đã nén {compacted_turns} lượt trước đó)]\n{summary}\n\nQUAN TRỌNG: TUYỆT ĐỐI KHÔNG đọc lại toàn bộ các tệp đã xem ở các bước trước. Dựa trực tiếp vào Bộ nhớ làm việc và kết quả test đã có để chốt công việc và gọi công cụ kết thúc (finish / approve_code / pass_ticket / fail_ticket) ngay khi đủ bằng chứng."
        )),
        tool_results: vec![],
    };
    let mut rebuilt = Vec::with_capacity(3 + tail.len());
    rebuilt.push(first);
    rebuilt.push(ack);
    rebuilt.push(memory);
    rebuilt.extend(tail);
    *messages = rebuilt;
    true
}

/// Intelligently compacts older middle turns of a long-running job into a dense Working Memory block.
/// Returns `true` when compaction succeeded.
pub async fn maybe_compact(
    store: &S,
    emp: &Employee,
    project: &Project,
    messages: &mut Vec<LlmMessage>,
    keep_recent_msgs: usize,
    cancel: &CancellationToken,
) -> bool {
    if emp.provider == ProviderId::Mock || cancel.is_cancelled() {
        return false;
    }
    // Need enough messages to make compaction worthwhile, and cut strictly on an odd index (Assistant boundary).
    let raw_split = messages.len().saturating_sub(keep_recent_msgs);
    let split = if raw_split % 2 == 0 { raw_split.saturating_sub(1) } else { raw_split };
    if split < 9 || split >= messages.len() {
        return false;
    }
    if !matches!(messages.get(split), Some(LlmMessage::Assistant { .. })) {
        return false;
    }
    let slice = &messages[1..split];
    let transcript = render_for_summary(slice, 22_000);
    if transcript.trim().is_empty() {
        return false;
    }
    let summarizer = light_employee(store, emp);
    let prompt = format!(
        "Tóm tắt các bước làm việc cũ dưới đây thành một khối BỘ NHỚ LÀM VIỆC ngắn gọn, giàu thông tin kỹ thuật để {} tiếp tục code/test mà không quên:\n\
- Các tệp đã đọc & tên hàm / class / hằng số / số dòng quan trọng đã tìm thấy.\n\
- Các tệp đã tạo hoặc đã sửa đổi trên đĩa.\n\
- Các lỗi / kết quả test đã gặp và những gì đã sửa xong vs chưa sửa.\n\
- Kế hoạch bước tiếp theo.\n\
Tối đa 15 gạch đầu dòng, giữ chính xác tên tệp/hàm/số dòng, tiếng Việt.\n\n--- Lịch sử các bước cũ ---\n{transcript}",
        emp.name
    );
    let meta = Meta {
        phase: "harness_compact",
        project_id: project.id.clone(),
        extra: String::new(),
        project_name: project.name.clone(),
        request: String::new(),
        ticket: None,
    };
    let req_msgs = vec![LlmMessage::User { text: Some(prompt), tool_results: vec![] }];
    let Ok(res) = llm::chat(
        store,
        LlmRequest {
            employee: &summarizer,
            system: "Bạn là bộ nén ngữ cảnh kỹ thuật (Agent Harness). Tóm tắt chính xác mọi chi tiết cần để code tiếp, bỏ văn nói thừa.",
            messages: &req_msgs,
            tools: &[],
            max_tokens: 1800,
            meta: &meta,
        },
        cancel,
    )
    .await
    else {
        return false;
    };
    store.add_usage(&emp.id, Some(&project.id), res.input_tokens, res.output_tokens);
    let summary = res.text.trim();
    if summary.chars().count() < 30 {
        return false;
    }
    if apply_compaction(messages, split, summary) {
        store.log_activity(
            &emp.id,
            Some(&project.id),
            "result",
            format!("Harness đã nén {} lượt cũ thành Bộ nhớ làm việc ({} ký tự)", (split - 1) / 2, summary.chars().count()),
        );
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{ToolCall, ToolResult};
    use serde_json::json;

    #[test]
    fn progress_ledger_records_and_bounds_entries() {
        let tmp = std::env::temp_dir().join(format!("ai_company_harness_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        for i in 1..=20 {
            record_entry(&tmp, &format!("Mốc {i}"), &format!("Nội dung bước {i}"));
        }
        let text = std::fs::read_to_string(tmp.join(PROGRESS_REL)).unwrap();
        assert!(!text.contains("Mốc 1\n"));
        assert!(text.contains("Mốc 20"));
        let count = text.split(ENTRY_SEP).skip(1).count();
        assert_eq!(count, MAX_PROGRESS_ENTRIES);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn compaction_preserves_alternation_and_tool_pairs() {
        let mut msgs = vec![LlmMessage::User { text: Some("task gốc".into()), tool_results: vec![] }];
        for i in 0..15 {
            msgs.push(LlmMessage::Assistant {
                text: String::new(),
                tool_calls: vec![ToolCall { id: format!("id_{i}"), name: "read_file".into(), input: json!({"path": "a.py"}) }],
                raw: None,
                raw_provider: None,
            });
            msgs.push(LlmMessage::User {
                text: None,
                tool_results: vec![ToolResult { id: format!("id_{i}"), content: format!("res_{i}"), is_error: false, images: vec![] }],
            });
        }
        // Total = 31 messages (index 0..=30). Cut at odd index 19 (keeps last 6 full turns: 19..=30).
        assert!(apply_compaction(&mut msgs, 19, "- Đã đọc a.py\n- Tiếp theo sửa b.py"));
        assert_eq!(msgs.len(), 3 + 12);
        // Verify strict User/Assistant alternation across entire rebuilt history.
        for (idx, m) in msgs.iter().enumerate() {
            let is_user = matches!(m, LlmMessage::User { .. });
            assert_eq!(is_user, idx % 2 == 0, "role mismatch at index {idx}");
        }
        // Verify first preserved assistant tool_call matches its immediately following tool_result.
        match (&msgs[3], &msgs[4]) {
            (LlmMessage::Assistant { tool_calls, .. }, LlmMessage::User { tool_results, .. }) => {
                assert_eq!(tool_calls[0].id, "id_9");
                assert_eq!(tool_results[0].id, "id_9");
            }
            _ => panic!("expected intact assistant/user tool pair"),
        }
    }
}
