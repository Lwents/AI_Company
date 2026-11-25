//! Free scripted provider so the whole company runs without API keys.
//! It builds a small to-do web app and lets the tester find one bug on purpose,
//! so the full Dev ⇄ Tester loop is exercised.
use super::{LlmMessage, LlmRequest, LlmResponse, Stop, ToolCall};
use crate::store::S;
use crate::types::TicketStatus;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn call(name: &str, input: Value) -> ToolCall {
    ToolCall { id: format!("mock_{}", SEQ.fetch_add(1, Ordering::Relaxed)), name: name.into(), input }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn index_html(title: &str) -> String {
    let t = esc(title);
    format!(
        r#"<!doctype html>
<html lang="vi">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{t}</title>
  <link rel="stylesheet" href="style.css" />
</head>
<body>
  <main class="card">
    <h1>{t}</h1>
    <form id="form">
      <input id="input" placeholder="Thêm việc cần làm…" autocomplete="off" />
      <button>Thêm</button>
    </form>
    <ul id="list"></ul>
    <p id="count"></p>
  </main>
  <script src="app.js"></script>
</body>
</html>
"#
    )
}

const STYLE: &str = r#"* { box-sizing: border-box; }
body { margin: 0; min-height: 100vh; display: grid; place-items: center; font-family: system-ui, sans-serif; background: #f3f5f9; color: #1d2433; }
.card { width: min(440px, 92vw); background: #fff; border-radius: 16px; padding: 24px; box-shadow: 0 10px 30px #0001; }
h1 { margin: 0 0 16px; font-size: 22px; }
form { display: flex; gap: 8px; }
input { flex: 1; padding: 10px 12px; border: 1px solid #d5dae3; border-radius: 10px; font-size: 15px; }
button { padding: 10px 14px; border: 0; border-radius: 10px; background: #4f6bed; color: #fff; cursor: pointer; }
ul { list-style: none; padding: 0; margin: 16px 0 0; }
li { display: flex; align-items: center; gap: 10px; padding: 10px 4px; border-bottom: 1px solid #eef0f4; }
li.done span { text-decoration: line-through; color: #98a1b3; }
li span { flex: 1; }
li .del { background: #fde8e8; color: #c0392b; padding: 4px 10px; }
#count { color: #6b7385; font-size: 13px; }
"#;

fn app_js(remove: bool, persist: bool) -> String {
    let storage = if persist {
        "const KEY = \"todos\";\nlet todos = JSON.parse(localStorage.getItem(KEY) || \"[]\");\nconst save = () => localStorage.setItem(KEY, JSON.stringify(todos));"
    } else {
        "let todos = [];\nconst save = () => {};"
    };
    let del = if remove {
        "\n    const del = document.createElement(\"button\");\n    del.className = \"del\";\n    del.textContent = \"Xóa\";\n    del.onclick = () => { todos = todos.filter((x) => x !== t); save(); render(); };\n    li.append(del);"
    } else {
        ""
    };
    format!(
        r#"const form = document.getElementById("form");
const input = document.getElementById("input");
const list = document.getElementById("list");
const count = document.getElementById("count");
{storage}

function render() {{
  list.innerHTML = "";
  for (const t of todos) {{
    const li = document.createElement("li");
    li.className = t.done ? "done" : "";
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = t.done;
    box.onchange = () => {{ t.done = box.checked; save(); render(); }};
    const text = document.createElement("span");
    text.textContent = t.text;
    li.append(box, text);{del}
    list.append(li);
  }}
  const left = todos.filter((t) => !t.done).length;
  count.textContent = todos.length ? left + " việc chưa xong / " + todos.length : "Chưa có việc nào.";
}}

form.onsubmit = (e) => {{
  e.preventDefault();
  const text = input.value.trim();
  if (!text) return;
  todos.push({{ text, done: false }});
  input.value = "";
  save();
  render();
}};

render();
"#
    )
}

fn script(store: &S, req: &LlmRequest<'_>) -> Option<(String, Vec<ToolCall>)> {
    let step = req.messages.iter().filter(|m| matches!(m, LlmMessage::Assistant { .. })).count();
    let meta = req.meta;
    let title = if meta.project_name.is_empty() { "Danh sách việc cần làm".to_string() } else { meta.project_name.clone() };
    let tk_title = meta.ticket.as_ref().map(|t| t.title.clone()).unwrap_or_default();
    let tk_key = meta.ticket.as_ref().map(|t| t.key.clone()).unwrap_or_default();
    let bounces = meta.ticket.as_ref().map(|t| t.bounces).unwrap_or(0);
    let is_persist = tk_title.to_lowercase().contains("localstorage");
    let s = |t: &str| t.to_string();

    let steps: Vec<(String, Vec<ToolCall>)> = match meta.phase {
        "plan" => vec![
            (s("Tôi đã nắm yêu cầu, viết kế hoạch rồi bàn giao cho BA."), vec![call("write_file", json!({
                "path": "docs/PLAN.md",
                "content": format!("# Kế hoạch: {title}\n\n## Yêu cầu của Chủ tịch\n{}\n\n## Mục tiêu\n- Một trang web chạy ngay trên trình duyệt, không cần cài đặt.\n- Thêm, đánh dấu hoàn thành và xóa việc.\n- Dữ liệu còn nguyên khi tải lại trang.\n\n## Phân công\n- BA: viết yêu cầu và ticket.\n- Dev: làm giao diện và logic.\n- Tester: kiểm tra theo tiêu chí nghiệm thu.\n", meta.request)
            }))]),
            (s(""), vec![call("finish", json!({ "summary": "Đã lập kế hoạch tại docs/PLAN.md, chia 2 hạng mục: giao diện và lưu dữ liệu." }))]),
        ],
        "requirements" => vec![
            (s("Đọc kế hoạch của CEO."), vec![call("read_file", json!({ "path": "docs/PLAN.md" }))]),
            (s("Viết tài liệu yêu cầu và tạo ticket."), vec![
                call("write_file", json!({ "path": "docs/REQUIREMENTS.md", "content": "# Yêu cầu\n\n## US-1 Quản lý việc\nLà người dùng, tôi muốn thêm, tick hoàn thành và xóa việc.\n\n## US-2 Lưu dữ liệu\nLà người dùng, tôi muốn danh sách còn nguyên khi tải lại trang.\n" })),
                call("create_ticket", json!({ "title": "Giao diện danh sách việc", "type": "feature",
                    "description": "Trang index.html với ô nhập, nút Thêm, danh sách việc có checkbox và nút Xóa, dòng đếm số việc.",
                    "acceptance": ["Thêm được việc mới", "Tick hoàn thành thì gạch ngang", "Mỗi việc có nút Xóa hoạt động"] })),
                call("create_ticket", json!({ "title": "Lưu danh sách vào localStorage", "type": "feature",
                    "description": "Lưu và đọc lại danh sách việc từ localStorage.",
                    "acceptance": ["Tải lại trang vẫn còn dữ liệu", "Không lỗi khi localStorage trống"] })),
            ]),
            (s(""), vec![call("finish", json!({ "summary": "Đã viết docs/REQUIREMENTS.md và tạo 2 ticket." }))]),
        ],
        "planning" => {
            let keys: Vec<String> = store.with(|st| {
                st.tickets.iter().filter(|t| t.project_id == meta.project_id).map(|t| t.key.clone()).collect()
            });
            let prio: Vec<ToolCall> = keys
                .iter()
                .enumerate()
                .map(|(i, k)| call("set_priority", json!({ "key": k, "priority": if i == 0 { 1 } else { 2 } })))
                .collect();
            vec![
                (s("Xem yêu cầu và backlog."), vec![call("read_file", json!({ "path": "docs/REQUIREMENTS.md" }))]),
                (s("Xếp ưu tiên: giao diện trước, lưu dữ liệu sau."), prio),
                (s(""), vec![call("write_file", json!({ "path": "docs/SPRINT.md", "content": format!("# Sprint 1\n\n## Mục tiêu\nRa bản dùng được của \"{title}\".\n\n## Thứ tự\n{}\n\n## Rủi ro\n- Dữ liệu localStorage hỏng → xử lý JSON lỗi.\n", keys.iter().map(|k| format!("1. {k}")).collect::<Vec<_>>().join("\n")) }))]),
                (s(""), vec![call("finish", json!({ "summary": "Đã xếp ưu tiên ticket và viết docs/SPRINT.md." }))]),
            ]
        }
        "design" => vec![
            (s("Thiết kế giao diện gọn, sáng."), vec![
                call("write_file", json!({ "path": "docs/DESIGN.md", "content": "# Thiết kế\n\n- Phong cách: thẻ trắng nổi trên nền xám nhạt.\n- Màu chính #4f6bed, chữ #1d2433, nền #f3f5f9, lỗi #c0392b.\n- Font: system-ui, tiêu đề 22px.\n- Bố cục: một cột, rộng tối đa 440px, căn giữa.\n- Trạng thái: việc xong gạch ngang màu xám; danh sách trống hiện \"Chưa có việc nào.\"\n" })),
                call("write_file", json!({ "path": "design/tokens.css", "content": ":root {\n  --primary: #4f6bed;\n  --text: #1d2433;\n  --bg: #f3f5f9;\n  --danger: #c0392b;\n  --radius: 10px;\n}\n" })),
            ]),
            (s(""), vec![call("finish", json!({ "summary": "Đã có docs/DESIGN.md và design/tokens.css." }))]),
        ],
        "standup" => vec![
            (s("Phân việc đầu ngày."), vec![call("message_colleague", json!({ "to_role": "dev", "text": "Làm ticket ưu tiên 1 (giao diện) trước, nhớ dùng màu trong design/tokens.css." }))]),
            (s(""), vec![call("finish", json!({ "summary": "Giao diện làm trước, lưu dữ liệu làm sau. Không ai bị vướng." }))]),
        ],
        "release" => vec![
            (s("Kiểm tra bản build."), vec![call("run_command", json!({ "command": "node --check app.js" }))]),
            (s("Viết hướng dẫn chạy."), vec![
                call("write_file", json!({ "path": "README.md", "content": format!("# {title}\n\nTrang web quản lý việc cần làm.\n\n## Chạy\nMở `index.html` bằng trình duyệt. Không cần cài đặt.\n") })),
                call("write_file", json!({ "path": "docs/RELEASE.md", "content": "# Phát hành 1.0\n\n- `node --check app.js`: đạt\n- Tệp đầy đủ: index.html, style.css, app.js\n" })),
            ]),
            (s(""), vec![call("finish", json!({ "summary": "Bản 1.0 chạy được, đã có README.md." }))]),
        ],
        "minutes" => {
            let task = req.messages.iter().find_map(|m| match m {
                LlmMessage::User { text: Some(t), .. } => Some(t.clone()),
                _ => None,
            }).unwrap_or_default();
            let file = task
                .split_whitespace()
                .find(|w| w.starts_with("docs/meetings/"))
                .map(|w| w.trim_end_matches([':', '.', ',']).to_string())
                .unwrap_or_else(|| "docs/meetings/bien-ban.md".into());
            vec![
                (s(""), vec![call("write_file", json!({ "path": file, "content": format!("# Biên bản họp — {title}\n\n- Thành phần: toàn công ty\n- Nội dung: thống nhất kế hoạch và phân công\n- Việc tiếp theo: theo docs/PLAN.md\n") }))]),
                (s(""), vec![call("finish", json!({ "summary": "Đã ghi biên bản." }))]),
            ]
        }
        "launch_legal" => vec![
            (s("Soạn điều khoản."), vec![
                call("write_file", json!({ "path": "legal/TERMS.md", "content": format!("# Điều khoản sử dụng — {title}\n\n1. Bạn dùng sản phẩm miễn phí ở gói cơ bản.\n2. Dữ liệu lưu trên trình duyệt của bạn.\n3. Không dùng sản phẩm cho mục đích trái pháp luật.\n") })),
                call("write_file", json!({ "path": "legal/PRIVACY.md", "content": "# Chính sách bảo mật\n\nChúng tôi không thu thập dữ liệu cá nhân. Danh sách việc chỉ nằm trong localStorage của trình duyệt.\n" })),
            ]),
            (s(""), vec![call("finish", json!({ "summary": "Đã có điều khoản và chính sách bảo mật." }))]),
        ],
        "launch_marketing" => vec![
            (s("Làm bộ ra mắt."), vec![
                call("write_file", json!({ "path": "marketing/landing.html", "content": format!("<!doctype html><html lang=\"vi\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{t}</title><style>body{{margin:0;font-family:system-ui;background:#0f172a;color:#e2e8f0}}.hero{{max-width:760px;margin:0 auto;padding:80px 24px;text-align:center}}h1{{font-size:44px;margin:0 0 12px}}p{{color:#94a3b8;font-size:18px}}a{{display:inline-block;margin:24px 8px 0;padding:14px 22px;border-radius:12px;text-decoration:none;font-weight:600}}.p{{background:#4f6bed;color:#fff}}.s{{border:1px solid #334155;color:#e2e8f0}}ul{{text-align:left;display:inline-block;color:#cbd5e1;line-height:1.9}}</style></head><body><div class=\"hero\"><h1>{t}</h1><p>Ghi lại việc cần làm, tick khi xong — đơn giản, nhanh, miễn phí.</p><ul><li>Thêm việc trong 1 giây</li><li>Đánh dấu hoàn thành, xóa dễ dàng</li><li>Dữ liệu còn nguyên khi tải lại</li></ul><br><a class=\"p\" href=\"../index.html\">Dùng thử ngay</a><a class=\"s\" href=\"/portal/{pid}/\">Liên hệ</a></div></body></html>\n", t = esc(&title), pid = meta.project_id) })),
                call("write_file", json!({ "path": "marketing/social-posts.md", "content": format!("# Bài đăng\n\n1. 🚀 Ra mắt {title}! Quản lý việc cần làm chỉ trong 1 giây.\n2. Bạn hay quên việc? Thử {title} — miễn phí.\n3. Tick ✔ — cảm giác hoàn thành thật đã!\n4. Không cần đăng ký, mở là dùng.\n5. Góp ý cho chúng tôi qua Cổng khách hàng nhé!\n") })),
                call("write_file", json!({ "path": "marketing/email.md", "content": format!("Tiêu đề: {title} chính thức ra mắt\n\nChào bạn,\nChúng tôi vừa ra mắt {title} — cách nhanh nhất để quản lý việc cần làm.\n") })),
                call("write_file", json!({ "path": "marketing/SEO.md", "content": "# SEO\n\n- Từ khóa: quản lý việc cần làm, todo list, ghi chú công việc\n- Mô tả: Ứng dụng việc cần làm miễn phí, đơn giản, không cần đăng ký.\n" })),
            ]),
            (s(""), vec![call("finish", json!({ "summary": "Đã có landing page, 5 bài mạng xã hội, email và SEO." }))]),
        ],
        "launch_sales" => vec![
            (s("Lên bảng giá."), vec![
                call("write_file", json!({ "path": "sales/pricing.md", "content": "# Bảng giá\n\n| Gói | Giá | Dành cho |\n|---|---|---|\n| Cá nhân | Miễn phí | 1 người |\n| Pro | 99.000 đ/tháng | Người dùng chuyên nghiệp |\n| Doanh nghiệp | 4.900.000 đ/năm (tối đa 25 người) | Công ty, nhóm |\n" })),
                call("write_file", json!({ "path": "sales/pitch.md", "content": "# Kịch bản tư vấn\n\n1. Hỏi: anh/chị dùng cho cá nhân hay nhóm? Bao nhiêu người?\n2. Giới thiệu gói phù hợp.\n3. Khách chê đắt → nhấn mạnh tiết kiệm thời gian, có dùng thử.\n" })),
            ]),
            (s(""), vec![call("finish", json!({ "summary": "Đã có bảng giá 3 gói và kịch bản tư vấn." }))]),
        ],
        "launch_support" => vec![
            (s("Soạn FAQ."), vec![
                call("write_file", json!({ "path": "support/FAQ.md", "content": "# Câu hỏi thường gặp\n\n**Dữ liệu lưu ở đâu?** Trong trình duyệt (localStorage) của máy bạn. Đổi máy hoặc xóa dữ liệu trình duyệt thì danh sách sẽ không còn.\n\n**Có mất phí không?** Gói cá nhân miễn phí.\n\n**Xóa việc thế nào?** Bấm nút Xóa bên phải mỗi việc.\n" })),
                call("write_file", json!({ "path": "support/playbook.md", "content": "# Quy trình CSKH\n\n- Phản hồi trong 15 phút.\n- Lỗi tái hiện được → chuyển kỹ thuật, hẹn báo lại.\n- Hỏi mua → chuyển Sales.\n" })),
            ]),
            (s(""), vec![call("finish", json!({ "summary": "Đã có FAQ và quy trình CSKH." }))]),
        ],
        "launch_finance" | "finance" => vec![
            (s("Lấy số liệu."), vec![call("company_finance", json!({}))]),
            (s(""), vec![call("write_file", json!({ "path": if meta.phase == "finance" { "finance/report-latest.md" } else { "finance/report.md" }, "content": "# Báo cáo tài chính\n\n- Chi phí AI: xem số liệu company_finance (Mock = 0 đ).\n- Doanh thu: theo đơn đã chốt.\n- Đề xuất: dùng Haiku cho CSKH để tiết kiệm.\n" }))]),
            (s(""), vec![call("finish", json!({ "summary": "Đã lập báo cáo tài chính. Chi phí AI đang ở mức thấp (Mock); đề xuất giữ Haiku cho CSKH khi chuyển sang model thật." }))]),
        ],
        "launch_hr" | "hr_review" => vec![
            (s("Xem KPI."), vec![call("company_kpi", json!({}))]),
            (s(""), vec![call("write_file", json!({ "path": "hr/review.md", "content": "# Đánh giá nhân sự\n\n- Dev và Tester phối hợp tốt, 1 lỗi được phát hiện và sửa trong 1 vòng.\n- Đề xuất: giữ nguyên đội hình.\n" }))]),
            (s(""), vec![call("finish", json!({ "summary": "Đội làm việc ổn định, chưa cần tuyển thêm." }))]),
        ],
        "campaign" => vec![
            (s("Lên kế hoạch chiến dịch."), vec![call("write_file", json!({ "path": "marketing/campaigns/chien-dich.md", "content": format!("# Chiến dịch\n\n- Mục tiêu: 1.000 người dùng mới\n- Kênh: Facebook, TikTok\n- Thông điệp: \"{title} — xong việc nhẹ đầu\"\n- Lịch: 3 bài/tuần trong 4 tuần\n") }))]),
            (s(""), vec![call("finish", json!({ "summary": "Đã lập kế hoạch chiến dịch 4 tuần trên Facebook và TikTok." }))]),
        ],
        "support" => {
            let x = meta.extra.to_lowercase();
            let fixed = x.contains("đã sửa xong");
            let escalated = x.contains("đã chuyển đội kỹ thuật");
            let bug = x.contains("loại bug") || x.contains("lỗi") || x.contains("không xóa") || x.contains("không mất");
            let second: Vec<ToolCall> = if fixed {
                vec![
                    call("reply_customer", json!({ "text": "Dạ đội kỹ thuật đã sửa xong lỗi anh/chị báo và cập nhật bản mới rồi ạ. Anh/chị tải lại trang và thử giúp em nhé. Cảm ơn anh/chị đã báo lỗi!" })),
                    call("resolve_case", json!({ "summary": "Đã sửa lỗi và báo khách." })),
                ]
            } else if escalated {
                vec![call("reply_customer", json!({ "text": "Dạ lỗi này đội kỹ thuật đang xử lý, có kết quả em báo ngay ạ." }))]
            } else if bug {
                vec![
                    call("reply_customer", json!({ "text": "Dạ em rất xin lỗi vì sự bất tiện. Em đã chuyển lỗi này cho đội kỹ thuật xử lý gấp, sửa xong em sẽ báo anh/chị ngay ạ." })),
                    call("escalate_bug", json!({ "title": "Khách không xóa được việc", "description": "Khách bấm Xóa nhưng việc vẫn còn sau khi tải lại trang.", "steps": "Thêm việc → bấm Xóa → tải lại trang" })),
                ]
            } else if x.contains("dark mode") || x.contains("chế độ tối") {
                vec![
                    call("reply_customer", json!({ "text": "Cảm ơn bạn đã góp ý! Em đã ghi nhận đề xuất chế độ tối gửi đội sản phẩm cho phiên bản tới ạ." })),
                    call("resolve_case", json!({ "summary": "Ghi nhận góp ý dark mode." })),
                ]
            } else {
                vec![
                    call("reply_customer", json!({ "text": "Dạ dữ liệu được lưu ngay trên trình duyệt của máy chị nên rất riêng tư. Nếu đổi máy hoặc xóa dữ liệu trình duyệt thì danh sách sẽ không còn ạ. Chị cần em hỗ trợ gì thêm không ạ?" })),
                    call("resolve_case", json!({ "summary": "Giải đáp nơi lưu dữ liệu." })),
                ]
            };
            vec![
                (s("Đọc FAQ."), vec![call("read_file", json!({ "path": "support/FAQ.md" }))]),
                (s(""), second),
                (s(""), vec![call("finish", json!({ "summary": "Đã xử lý hồ sơ khách." }))]),
            ]
        }
        "sales" => {
            let customer = meta
                .extra
                .lines()
                .find_map(|l| l.strip_prefix("Khách hàng: "))
                .map(|l| l.split(" (").next().unwrap_or(l).to_string())
                .unwrap_or_else(|| "Khách hàng".into());
            let x = meta.extra.to_lowercase();
            let buy = x.contains("đăng ký") || x.contains("mua") || x.contains("ok");
            let mut second = vec![call("reply_customer", json!({ "text": "Dạ với 20 nhân viên, gói Doanh nghiệp phù hợp nhất: 4.900.000 đ/năm cho tối đa 25 người, có hỗ trợ ưu tiên ạ. Bên mình đồng ý thì em làm thủ tục kích hoạt ngay." }))];
            if buy {
                second.push(call("propose_deal", json!({ "customer": customer, "plan": "Doanh nghiệp (25 người, 1 năm)", "amount_vnd": 4900000 })));
            }
            vec![
                (s("Xem bảng giá."), vec![call("read_file", json!({ "path": "sales/pricing.md" }))]),
                (s(""), second),
                (s(""), vec![
                    call("reply_customer", json!({ "text": "Cảm ơn quý công ty đã tin chọn! Em gửi hướng dẫn kích hoạt qua email trong hôm nay ạ." })),
                    call("resolve_case", json!({ "summary": "Đã tư vấn và chốt gói Doanh nghiệp." })),
                ]),
                (s(""), vec![call("finish", json!({ "summary": "Đã tư vấn khách doanh nghiệp." }))]),
            ]
        }
        "architecture" => vec![
            (s(""), vec![call("write_file", json!({ "path": "docs/ARCHITECTURE.md", "content": "# Kiến trúc\n\nHTML + CSS + JavaScript thuần, không build. Tệp: index.html, style.css, app.js.\n" }))]),
            (s(""), vec![call("finish", json!({ "summary": "Chọn HTML/CSS/JS thuần, không cần build." }))]),
        ],
        "implement" => {
            let persist_done = store.with(|st| {
                st.tickets.iter().any(|t| t.title.to_lowercase().contains("localstorage") && t.status == TicketStatus::Done)
            });
            let fix_round = bounces > 0;
            // Bugs reported after release: patch app.js with the full, correct version.
            let hotfix = tk_title.starts_with('[');
            let mut writes = vec![];
            if hotfix {
                writes.push(call("write_file", json!({ "path": "app.js", "content": app_js(true, true) })));
            } else if !is_persist && !fix_round {
                writes.push(call("write_file", json!({ "path": "index.html", "content": index_html(&title) })));
                writes.push(call("write_file", json!({ "path": "style.css", "content": STYLE })));
            }
            if !hotfix {
                writes.push(call("write_file", json!({ "path": "app.js", "content": app_js(fix_round || is_persist, is_persist || persist_done) })));
            }
            vec![
                (s("Xem các tệp hiện có."), vec![call("list_files", json!({}))]),
                (if fix_round { s("Sửa lỗi Tester báo: thêm nút Xóa.") } else { s("Viết code cho ticket.") }, writes),
                (s("Tự kiểm tra cú pháp."), vec![call("run_command", json!({ "command": "node --check app.js" }))]),
                (s(""), vec![call("finish", json!({ "summary": format!("Đã xong {tk_key}: {tk_title}.") }))]),
            ]
        }
        "review" => vec![
            (s(""), vec![call("read_file", json!({ "path": "app.js" }))]),
            (s(""), vec![call("approve_code", json!({ "comment": "Code gọn, đạt." }))]),
        ],
        "test" => {
            let fail = tk_title.contains("Giao diện") && bounces == 0;
            vec![
                (s("Đọc code để đối chiếu tiêu chí."), vec![call("read_file", json!({ "path": "app.js" }))]),
                (s("Chạy kiểm tra cú pháp."), vec![call("run_command", json!({ "command": "node --check app.js" }))]),
                if fail {
                    (s("Phát hiện thiếu chức năng."), vec![call("fail_ticket", json!({ "bug_report": "Không thấy nút Xóa trên từng việc (tiêu chí 3 chưa đạt). Tái hiện: thêm 1 việc, không có cách xóa." }))])
                } else {
                    (s(""), vec![call("pass_ticket", json!({ "summary": "Tất cả tiêu chí nghiệm thu đạt." }))])
                },
            ]
        }
        "report" => vec![
            (s(""), vec![call("list_files", json!({}))]),
            (s(""), vec![call("write_file", json!({ "path": "docs/REPORT.md", "content": format!("# Báo cáo: {title}\n\nĐã hoàn thành trang quản lý việc cần làm.\n\n- Thêm / tick / xóa việc\n- Lưu bằng localStorage\n\nMở index.html để dùng thử.\n") }))]),
            (s(""), vec![call("finish", json!({ "summary": format!("Kính gửi Chủ tịch: dự án \"{title}\" đã xong. Sản phẩm là trang web thêm/tick/xóa việc, dữ liệu lưu trên trình duyệt. Tester đã phát hiện và Dev đã sửa 1 lỗi (thiếu nút Xóa). Mời anh/chị mở tab Sản phẩm để xem thử.") }))]),
        ],
        "meeting_opinion" => vec![(s(""), vec![call("finish", json!({ "summary": "Đồng ý hướng đi chung; đề xuất làm bản đơn giản chạy được trước rồi mới mở rộng." }))])],
        "meeting_ballot" => vec![
            (s(""), vec![call("write_file", json!({ "path": "docs/meetings/kickoff/BALLOT.md", "content": "Không có vấn đề cần bỏ phiếu\n" }))]),
            (s(""), vec![call("finish", json!({ "summary": "Mọi người đã đồng thuận." }))]),
        ],
        "plan_decide" => vec![
            (s(""), vec![call("write_file", json!({ "path": "docs/DECISIONS.md", "content": "# Quyết định họp khởi động\n\n- Chấp nhận: làm bản đơn giản chạy được trước, mở rộng sau.\n" }))]),
            (s(""), vec![call("finish", json!({ "summary": "Giữ kế hoạch, ưu tiên bản đơn giản chạy được trước." }))]),
        ],
        _ => vec![(s(""), vec![call("finish", json!({ "summary": "Xong." }))])],
    };
    steps.into_iter().nth(step)
}

pub async fn chat(store: &S, req: &LlmRequest<'_>) -> Result<LlmResponse, String> {
    let base = store.with(|s| s.settings.mock_delay_ms) as f64;
    let delay = base * (0.6 + fastrand::f64() * 0.8);
    tokio::time::sleep(Duration::from_millis(delay as u64)).await;
    let (text, calls) = script(store, req).unwrap_or_else(|| (String::new(), vec![call("finish", json!({ "summary": "Hoàn tất." }))]));
    let in_chars = req.system.len() + req.messages.len() * 400;
    let out_chars = text.len() + calls.iter().map(|c| c.input.to_string().len()).sum::<usize>();
    Ok(LlmResponse {
        stop: if calls.is_empty() { Stop::End } else { Stop::Tool },
        text,
        tool_calls: calls,
        input_tokens: (in_chars / 4) as u64,
        output_tokens: (out_chars / 4) as u64,
        raw: None,
        raw_provider: None,
    })
}
