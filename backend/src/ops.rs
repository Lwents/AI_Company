//! Live operations for launched products: customer cases (support / sales), automatic maintenance
//! of bugs customers report, and on-demand business work (campaigns, finance, HR reviews).
use crate::agent::{self, Job};
use crate::store::{new_id, now_ms, S};
use crate::types::*;
use crate::workflow::{self, first, handoff};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub fn spawn(store: S) {
    tokio::spawn(async move {
        loop {
            tick(&store);
            tokio::time::sleep(Duration::from_millis(1200)).await;
        }
    });
}

fn tick(store: &S) {
    let live: Vec<Project> = store.with(|s| s.projects.iter().filter(|p| p.status == ProjectStatus::Operating).cloned().collect());
    for p in live {
        // Customer cases waiting for an answer.
        let open: Vec<String> = store.with(|s| {
            s.cases.iter().filter(|c| c.project_id == p.id && c.status == CaseStatus::Open).map(|c| c.id.clone()).collect()
        });
        for cid in open {
            let key = format!("case:{cid}");
            if !store.try_busy(&key) {
                continue;
            }
            // Assign now (not inside the task) so cases spread across the team.
            let Some(c) = store.case(&cid) else {
                store.done_busy(&key);
                continue;
            };
            let roles: &[Role] = if c.kind == CaseKind::Sales { &[Role::Sales, Role::Support, Role::Ceo] } else { &[Role::Support, Role::Sales, Role::Ceo] };
            let Some(emp) = pick(store, roles, c.assignee.as_deref()) else {
                store.done_busy(&key);
                continue;
            };
            store.update_case(&cid, |c| {
                c.status = CaseStatus::InProgress;
                c.assignee = Some(emp.id.clone());
            });
            let store = store.clone();
            let pid = p.id.clone();
            let cancel = store.ops_token(&pid);
            tokio::spawn(async move {
                let r = handle_case(&store, &pid, &cid, &emp, &cancel).await;
                if let Err(e) = r {
                    if e == "aborted" {
                        store.update_case(&cid, |c| {
                            if c.status == CaseStatus::InProgress {
                                c.status = CaseStatus::Open;
                            }
                        });
                    } else {
                        store.update_case(&cid, |c| {
                            c.status = CaseStatus::WaitingCustomer;
                            c.messages.push(CaseMsg { from: "system".into(), text: format!("Lỗi khi xử lý: {e}"), ts: now_ms() });
                        });
                    }
                }
                store.done_busy(&key);
            });
        }
        // Bugs reported by customers (or DevOps) → maintenance.
        let pending = store.tickets_of(&p.id).iter().any(|t| t.status == TicketStatus::Todo);
        let key = format!("maint:{}", p.id);
        if pending && store.try_busy(&key) {
            let store = store.clone();
            let pid = p.id.clone();
            let cancel = store.ops_token(&pid);
            tokio::spawn(async move {
                if let Err(e) = maintenance(&store, &pid, &cancel).await {
                    if e != "aborted" {
                        store.post_message(Some(&pid), "system", "chairman", format!("Bảo trì gặp lỗi: {e}"), "system");
                    }
                }
                store.done_busy(&key);
            });
        }
    }
}

/// Who handles a case: the same person as before if still on staff (customers like continuity),
/// otherwise whoever in the first staffed role holds the fewest cases in progress.
fn pick(store: &S, roles: &[Role], previous: Option<&str>) -> Option<Employee> {
    for r in roles {
        let staff = store.staff(*r);
        if staff.is_empty() {
            continue;
        }
        if let Some(prev) = previous.and_then(|id| staff.iter().find(|e| e.id == id)) {
            return Some(prev.clone());
        }
        return store.with(|s| {
            staff
                .iter()
                .min_by_key(|e| s.cases.iter().filter(|c| c.status == CaseStatus::InProgress && c.assignee.as_deref() == Some(&e.id)).count())
                .cloned()
        });
    }
    None
}

async fn handle_case(store: &S, pid: &str, cid: &str, emp: &Employee, cancel: &CancellationToken) -> Result<(), String> {
    let c = store.case(cid).ok_or("Hồ sơ không tồn tại")?;
    let sales = c.kind == CaseKind::Sales;
    let transcript = agent::case_transcript(store, &c);
    let task = if sales {
        format!(
            "Bạn đang tư vấn cho một khách hàng tiềm năng.\n\n{transcript}\nĐọc sales/pricing.md, sales/pitch.md, README.md nếu có. Trả lời khách bằng reply_customer: hỏi rõ nhu cầu, giới thiệu gói phù hợp kèm giá. Chỉ khi khách nói rõ muốn mua / đăng ký gói nào thì gọi propose_deal. Khi việc tư vấn đã xong (khách đã mua hoặc từ chối) thì resolve_case. Cuối cùng gọi finish."
        )
    } else {
        format!(
            "Bạn đang xử lý một hồ sơ chăm sóc khách hàng.\n\n{transcript}\nĐọc support/FAQ.md, support/playbook.md, README.md, docs/ nếu cần. Trả lời khách bằng reply_customer (lịch sự, ngắn gọn, đúng sự thật).\n- Nếu khách gặp lỗi sản phẩm có thật: xin lỗi, gọi escalate_bug, hẹn sẽ báo lại khi sửa xong.\n- Nếu hệ thống báo đội kỹ thuật đã sửa xong: báo khách, mời thử lại, rồi resolve_case.\n- Nếu đã giải đáp xong: resolve_case.\n- Câu hỏi hay gặp mà FAQ chưa có thì bổ sung vào support/FAQ.md.\n- Khách hỏi mua hàng thì tư vấn theo sales/pricing.md.\nCuối cùng gọi finish."
        )
    };
    let r = agent::run(
        store,
        Job { employee_id: emp.id.clone(), project_id: pid.into(), phase: if sales { "sales" } else { "support" }, ticket_id: None, case_id: Some(cid.into()), task },
        cancel,
    )
    .await;
    store.update_case(cid, |c| {
        if c.status == CaseStatus::InProgress {
            c.status = CaseStatus::WaitingCustomer;
        }
    });
    r.map(|_| ())
}

/// Fix what customers reported: devs → reviewer → tester → DevOps check, then tell support.
async fn maintenance(store: &S, pid: &str, cancel: &CancellationToken) -> Result<(), String> {
    let p = store.project(pid).ok_or("Dự án không tồn tại")?;
    for round in 0..3 {
        workflow::run_dev_workers(store, pid, cancel).await?;
        let p = store.project(pid).unwrap_or(p.clone());
        let more = workflow::release_check(store, &p, round < 2, cancel).await?;
        if !more {
            break;
        }
    }
    // Close the loop with customers whose bug is fixed.
    let fixed: Vec<(String, String, Option<String>)> = store.with(|s| {
        s.cases
            .iter()
            .filter(|c| c.project_id == pid && c.status == CaseStatus::Escalated)
            .filter_map(|c| {
                let t = s.tickets.iter().find(|t| Some(&t.id) == c.ticket_id.as_ref())?;
                (t.status == TicketStatus::Done).then(|| (c.id.clone(), t.key.clone(), t.assignee.clone()))
            })
            .collect()
    });
    for (cid, key, dev) in fixed {
        store.update_case(&cid, |c| {
            c.status = CaseStatus::Open;
            c.messages.push(CaseMsg {
                from: "system".into(),
                text: format!("Đội kỹ thuật đã sửa xong lỗi {key}, đã kiểm thử và phát hành. Hãy báo cho khách."),
                ts: now_ms(),
            });
        });
        if let (Some(dev), Some(sup)) = (dev, first(store, &[Role::Support, Role::Sales])) {
            handoff(store, pid, &dev, &sup.id, format!("{key} đã sửa xong, nhờ báo khách."));
        }
    }
    Ok(())
}

/// On-demand business work from the Operations tab.
pub fn run_business(store: &S, pid: &str, action: &str, brief: &str) -> Result<String, String> {
    let p = store.project(pid).ok_or("Dự án không tồn tại")?;
    let day = chrono_day();
    let (roles, phase, task): (&[Role], &'static str, String) = match action {
        "campaign" => (
            &[Role::Marketing, Role::Sales, Role::Ceo],
            "campaign",
            format!(
                "Chủ tịch giao chiến dịch marketing cho \"{}\":\n{}\n\nĐọc marketing/ và sales/pricing.md hiện có. Lập kế hoạch chiến dịch vào marketing/campaigns/{day}.md (mục tiêu, khách hàng mục tiêu, thông điệp, kênh, lịch đăng, ngân sách gợi ý, chỉ số đo lường) và viết sẵn nội dung đăng (bài mạng xã hội, email, banner text). Cập nhật marketing/landing.html nếu chiến dịch cần. Xong gọi finish.",
                p.name,
                if brief.trim().is_empty() { "(Chủ tịch không ghi chú — tự đề xuất chiến dịch phù hợp nhất lúc này.)" } else { brief }
            ),
        ),
        "finance" => (
            &[Role::Accountant, Role::Ceo],
            "finance",
            format!("Lập báo cáo tài chính ngày {day}. Gọi company_finance để lấy số liệu thật. Viết finance/report-{day}.md: tổng chi phí AI (USD và VNĐ), chi phí theo nhân viên và dự án, doanh thu từ đơn đã chốt, lãi/lỗ, nhân viên tốn kém nhất, đề xuất tiết kiệm cụ thể. {brief} Xong gọi finish với tóm tắt 2–3 câu cho Chủ tịch."),
        ),
        "hr" => (
            &[Role::Hr, Role::Ceo],
            "hr_review",
            format!("Đánh giá nhân sự định kỳ ngày {day}. Gọi company_kpi. Viết hr/review-{day}.md: xếp hạng, điểm mạnh/yếu từng người, ai nên đổi model mạnh hơn hoặc rẻ hơn, có cần tuyển thêm không (nếu thật sự thiếu người thì propose_hire). {brief} Xong gọi finish với tóm tắt cho Chủ tịch."),
        ),
        _ => return Err("Hành động không hợp lệ".into()),
    };
    let emp = first(store, roles).ok_or("Công ty chưa có người cho việc này")?;
    let key = format!("{action}:{pid}");
    if !store.try_busy(&key) {
        return Err("Việc này đang được làm, chờ xong đã".into());
    }
    let cancel = store.ops_token(pid);
    let store = store.clone();
    let pid = pid.to_string();
    let who = emp.name.clone();
    tokio::spawn(async move {
        store.post_message(Some(&pid), "chairman", &emp.id, task.lines().next().unwrap_or("").to_string(), "chat");
        let r = agent::run(&store, Job { employee_id: emp.id.clone(), project_id: pid.clone(), phase, ticket_id: None, case_id: None, task }, &cancel).await;
        match r {
            Ok(r) => {
                store.kpi(&emp.id, |k| k.campaigns += 1);
                store.post_message(Some(&pid), &emp.id, "chairman", workflow::summary_of(&r.outcome), "report");
                store.emit(ServerEvent::Handoff { from: emp.id.clone(), to: "chairman".into(), text: "Báo cáo".into() });
            }
            Err(e) if e != "aborted" => {
                store.post_message(Some(&pid), "system", "chairman", format!("{} gặp lỗi: {e}", emp.name), "system");
            }
            _ => {}
        }
        store.done_busy(&key);
    });
    Ok(format!("Đã giao cho {who}"))
}

fn chrono_day() -> String {
    // yyyy-mm-dd from the system clock (UTC is fine for file names).
    let secs = now_ms() / 1000;
    let days = (secs / 86_400) as i64;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

/// A new customer contact (from the portal or the simulator).
pub fn new_case(store: &S, pid: &str, kind: CaseKind, customer: &str, contact: &str, subject: &str, message: &str) -> SupportCase {
    let token = format!("{}{}", new_id(), new_id());
    let c = SupportCase {
        id: new_id(),
        project_id: pid.into(),
        kind,
        status: CaseStatus::Open,
        customer: customer.trim().chars().take(60).collect(),
        contact: contact.trim().chars().take(100).collect(),
        subject: subject.trim().chars().take(140).collect(),
        messages: vec![CaseMsg { from: "customer".into(), text: message.trim().chars().take(4000).collect(), ts: now_ms() }],
        assignee: None,
        ticket_id: None,
        rating: None,
        token,
        created_at: now_ms(),
        updated_at: now_ms(),
    };
    store.insert_case(c.clone());
    let roles: &[Role] = if kind == CaseKind::Sales { &[Role::Sales, Role::Support] } else { &[Role::Support, Role::Sales] };
    if let Some(e) = first(store, roles) {
        store.emit(ServerEvent::Handoff { from: "customer".into(), to: e.id, text: "☎ Khách hàng!".into() });
    }
    c
}

/// Sample customers for trying the operations flow without real traffic.
pub fn simulate_customers(store: &S, pid: &str) -> usize {
    let samples: [(CaseKind, &str, &str, &str, &str); 4] = [
        (CaseKind::Support, "Chị Hoa", "hoa@example.com", "Hỏi cách dùng", "Cho mình hỏi dữ liệu của mình được lưu ở đâu? Đổi máy thì có còn không?"),
        (CaseKind::Bug, "Anh Minh", "0900000001", "Không xóa được", "Mình bấm nút Xóa nhưng việc vẫn không mất, tải lại trang vẫn còn. Lỗi này khó chịu quá."),
        (CaseKind::Sales, "Công ty Sao Mai", "saomai@example.com", "Báo giá cho 20 nhân viên", "Bên mình có 20 nhân viên muốn dùng chung, cho mình xin báo giá gói doanh nghiệp. Ok thì bên mình đăng ký luôn."),
        (CaseKind::Support, "Bạn Long", "", "Góp ý", "App dễ dùng lắm, có thể thêm chế độ tối (dark mode) không ạ?"),
    ];
    let n = samples.len();
    for (kind, name, contact, subject, msg) in samples {
        new_case(store, pid, kind, name, contact, subject, msg);
    }
    n
}
