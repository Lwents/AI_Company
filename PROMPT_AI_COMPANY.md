# Prompt xây dựng "AI Company": công ty phần mềm do các nhân viên AI vận hành

> Dán toàn bộ tệp này cho một AI lập trình (Claude Code, Cursor, Copilot…). Làm theo thứ tự các giai đoạn ở cuối tệp, mỗi giai đoạn chạy được rồi mới làm tiếp. Muốn làm "công ty" ở lĩnh vực khác thì xem mục 14.

---

## 1. Mục tiêu

Xây một ứng dụng chạy trên máy cá nhân, mô phỏng **một công ty phần mềm thật, trong đó mọi nhân viên là AI**:

- Người dùng là **Chủ tịch**: giao dự án bằng một câu yêu cầu, trả lời câu hỏi, duyệt các mốc quan trọng.
- Các nhân viên AI (CEO, PM, BA, Tech Lead, Data Engineer, Designer, Dev, Reviewer, Tester, DevOps, Security, Thư ký, Marketing, Sales, CSKH, Nhân sự, Kế toán, Pháp chế) **phối hợp theo quy trình** để làm ra sản phẩm phần mềm chạy được, kèm tài liệu chuyên nghiệp.
- Giao diện là **văn phòng pixel-art động**: nhân viên ngồi bàn, đi lại đưa tài liệu, họp ở phòng họp, có bong bóng thoại. Bên cạnh là các bảng điều khiển: Dự án, Kanban, Chat, Nhân sự, KPI, Chi phí, Duyệt, Sản phẩm, Vận hành, Cài đặt.
- Sản phẩm đầu ra **đạt chuẩn thương mại**: code chia module, có test, có kiểm định, có tài liệu đặc tả kèm ảnh chụp.

## 2. Công nghệ

- **Backend:** Rust (tokio, axum có WebSocket, serde, reqwest, zip). Một tệp chạy duy nhất. Chỉ lắng nghe `127.0.0.1`.
- **Frontend:** React + TypeScript + Vite, vẽ văn phòng bằng Canvas 2D (pixel art, font VT323, bảng màu Sweetie-16, phóng to bằng sharp-bilinear).
- **Lưu trữ:** một tệp `data/state.json` (ghi lại sau mỗi thay đổi). Mã nguồn các dự án nằm trong `workspace/<slug>/`.
- **Model AI:** mọi nhà cung cấp tương thích OpenAI Chat Completions (OpenAI, OpenRouter, Gemini, Ollama, proxy tùy chỉnh) và Anthropic. Mỗi nhân viên chọn nhà cung cấp, model, mức suy nghĩ và giá token riêng. Có provider **Mock** chạy theo kịch bản để thử miễn phí.
- **Khóa API** chỉ nằm trong `.env`, không bao giờ đưa vào code hay vào môi trường lệnh mà nhân viên AI chạy.

## 3. Kiến trúc thư mục

```
backend/src/
  main.rs        khởi động, đọc .env, phục vụ web/dist
  api.rs         REST + WebSocket (sự kiện thời gian thực)
  store.rs       trạng thái công ty, khóa, phát sự kiện, hàng đợi duyệt
  types.rs       kiểu dữ liệu (khớp 1-1 với web/src/api/types.ts)
  config.rs      vai trò, mô tả nghề, giá model, chuẩn cấu trúc repo
  agent.rs       vòng lặp một nhân viên làm một việc (tool-use loop)
  tools.rs       công cụ của nhân viên + phân quyền theo vai
  workflow.rs    quy trình dự án theo các bước (mục 6)
  ops.rs         vận hành sau ra mắt (CSKH, Sales, sửa lỗi khách báo)
  portal.rs      cổng khách hàng công khai
  llm/           openai.rs, anthropic.rs, mock.rs
  mcp.rs         MCP client (stdio JSON-RPC)
  skills.rs      nạp sổ tay nghề skills/<vai>.md
  gitflow.rs     Git cho từng dự án
  history.rs     lịch sử phiên bản (.zip) + tải mã nguồn
  import.rs      nhận mã nguồn có sẵn (.zip, Git, thư mục trên máy)
web/src/
  app/ api/ store/ shared/ styles/
  features/<tính năng>/   projects, kanban, chat, staff, kpi, costs, approvals,
                          product, ops, settings, office (engine/ render/ sprites/)
skills/*.md      sổ tay nghề theo vai (người dùng tự sửa được)
mcp.json         cấu hình MCP server (giống định dạng của Claude, thêm roles/hint/exclude)
```

Quy tắc frontend: chia theo tính năng; import bằng `@/`; `useEffect` luôn dùng thân khối; không dùng `confirm()` / `alert()` của trình duyệt mà dùng hộp thoại pixel trong app; có ErrorBoundary.

## 4. Dữ liệu chính

- **Employee:** id, tên, vai, provider, model, effort, giá token vào/ra, baseUrl, chỉ dẫn thêm, màu, đang làm hay đã nghỉ.
- **Project:** id, tên, slug, yêu cầu, danh sách yêu cầu sửa, bước hiện tại, trạng thái, ngân sách và đã chi, báo cáo, lỗi, các bộ đếm vòng (phát hành, duyệt giao diện, kiểm định, bảo mật, tự chạy lại), danh sách phiên bản, có phải mã nguồn có sẵn không, nguồn, thư mục làm việc.
- **Ticket:** mã T-n, tiêu đề, mô tả, tiêu chí nghiệm thu, độ ưu tiên, trạng thái (todo / in_progress / review / testing / done / blocked), người làm, số lần bị trả lại, ghi chú.
- Ngoài ra: **Message** (chat, bàn giao, báo cáo, câu hỏi), **Approval** (lệnh, ngân sách, câu hỏi, chốt đơn, tuyển người), **Activity** (nhật ký công cụ), **Usage** (token, chi phí), **KPI**, **Case** (yêu cầu của khách), **Deal**, **Settings**.

## 5. Nhân viên AI làm việc thế nào

- Mỗi việc là một **Job** (nhân viên, dự án, bước, ticket, nội dung giao). Vòng lặp: gọi model, model gọi công cụ, trả kết quả, lặp tới khi model gọi **công cụ kết thúc** (finish, pass_ticket, fail_ticket, approve_code, request_changes), hoặc vượt số bước tối đa.
- **Lời nhắc hệ thống:** mô tả nghề, tên, thư mục dự án, quy tắc chung, sổ tay nghề (`skills/common.md` + `skills/<vai>.md`), **nguyên tắc dự án** (`docs/CONSTITUTION.md` nếu có), gợi ý dùng MCP, link bản xem thử.
- **Công cụ có sẵn:** list_files, read_file, write_file (phân quyền thư mục theo vai), run_command (chỉ một số vai; lệnh nguy hiểm bị chặn; lệnh ngoài danh sách an toàn phải xin Chủ tịch duyệt, trừ khi bật tự cho phép; xóa khóa API khỏi môi trường; đặt `GIT_CEILING_DIRECTORIES`), message_colleague, ask_chairman (CEO, hoặc PM/BA ở bước tư vấn và hỏi làm rõ), create_ticket, set_priority, report_issue, các công cụ CSKH / Sales / Tài chính / Nhân sự.
- **MCP:** mỗi Job tự khởi động các MCP server hợp với vai (trong thư mục dự án) và tắt cả cây tiến trình khi xong:
  - `@playwright/mcp`: trình duyệt Edge headless. **Ảnh chụp màn hình được gửi lại cho model** (chỉ giữ 2 ảnh mới nhất trong ngữ cảnh). Chặn `browser_run_code_unsafe`.
  - `@upstash/context7-mcp`: tra tài liệu thư viện mới nhất.
  - `@modelcontextprotocol/server-sequential-thinking`
  - `@modelcontextprotocol/server-memory`: bộ nhớ chung của công ty qua các dự án.
  - Ghim phiên bản cụ thể. Provider Mock không khởi động MCP.
- **An toàn:** đường dẫn luôn nằm trong thư mục dự án (chặn `..`, đường dẫn tuyệt đối); một nhân viên chỉ làm một việc một lúc; giới hạn số lượt gọi model đồng thời; có ngân sách theo dự án (vượt thì xin Chủ tịch).
- **Câu trả lời bị cắt** (quá dài): không ghi tệp dở dang, bắt làm lại ngắn hơn. Ghi cùng một tệp họp từ 3 lần liên tiếp thì nhắc dừng.

## 6. Quy trình một dự án (mỗi bước chạy được lại, bỏ qua bước của vai không có người)

1. **Nhận mã nguồn (tùy chọn):** làm mới, tải `.zip` (chặn đường dẫn trượt ra ngoài, bỏ node_modules/dist, gỡ lớp thư mục bọc ngoài), link Git (`git clone --depth 1`, chỉ nhận https / git@), hoặc **thư mục trên máy** (sửa trực tiếp tại chỗ; thư mục chưa có Git thì lưu bản gốc v1 trước). Tech Lead đọc hiểu code và viết `docs/EXISTING-CODE.md`. Quy tắc: không viết lại từ đầu, giữ công nghệ và phong cách code sẵn có.
2. **PM tư vấn công nghệ:** khách không rành kỹ thuật. PM đưa 2–3 phương án bằng lời dễ hiểu (chạy trên thiết bị nào, có cần máy chủ không, chi phí hằng tháng, sau này sửa có dễ không) kèm phương án đề xuất, hỏi Chủ tịch một lần, rồi ghi `docs/TECH-CHOICE.md`.
3. **Họp khởi động:**
   - CEO viết bản nháp `PLAN.md`.
   - 13–14 trưởng bộ phận **góp ý** (vòng 1) rồi **phản biện** nhau (vòng 2…), mỗi người một tệp, chạy song song.
   - CEO lập **phiếu bầu** (`## V1: … / - A: …`) cho các vấn đề còn tranh cãi.
   - Mọi người **bỏ phiếu** (`V1: B — lý do`). **Code đếm phiếu**, hòa thì theo phiếu CEO.
   - CEO **bắt buộc chốt theo kết quả bầu**, viết `DECISIONS.md` và `PLAN.md` bản cuối, rồi viết **`CONSTITUTION.md`** (5–10 nguyên tắc PHẢI / KHÔNG ĐƯỢC).
   - Thư ký ghi biên bản. Cuộc họp làm tiếp đúng chỗ dừng nếu bị ngắt.
4. **BA hỏi làm rõ:** tối đa 5 câu, mỗi câu có lựa chọn gợi ý và đề xuất, hỏi Chủ tịch một lần, ghi `CLARIFICATIONS.md`. Sau đó viết **`REQUIREMENTS.md` theo mẫu chuẩn**: user story P1/P2/P3, kịch bản Cho trước / Khi / Thì, ca biên, FR-001…, tiêu chí thành công đo được SC-001…; tạo 2–8 ticket, **mỗi ticket thuộc đúng một trang hoặc module** và ghi mã FR.
5. **PM** xếp sprint (độ ưu tiên, phụ thuộc, rủi ro).
6. **Designer:** viết `DESIGN.md`, `design/tokens.css` (nguồn duy nhất), `design/mockup.html`; tự mở bằng trình duyệt chụp ở 1280px và 375px rồi sửa tới khi đẹp. **Chủ tịch duyệt bản mẫu** (trả lời "ok" hoặc góp ý; tối đa 2 vòng sửa).
7. **Tech Lead:** viết `ARCHITECTURE.md` theo **chuẩn chia trang / module** (src/app, src/pages/<trang>, src/modules/<module> có index công khai, src/shared; quy tắc phụ thuộc pages → modules → shared), có bảng module, ADR trong `docs/adr/`, mục "Kiểm tra Constitution", và ticket "Khởi tạo khung dự án".
8. **Data Engineer:** viết `DATA.md` (mô hình dữ liệu, lưu trữ, phiên bản schema và cách chuyển dữ liệu cũ, sao lưu, quyền riêng tư, chỉ số) và các thư mục `data/schema|migrations|seeds`.
9. **Kiểm tra nhất quán:** BA đối chiếu FR ↔ ticket ↔ tài liệu ↔ nguyên tắc, viết `ANALYSIS.md`, bổ sung ticket cho FR còn thiếu.
10. **Lập trình:** nhiều Dev làm song song. Mỗi ticket đi Dev → Reviewer (xem `git diff`) → Tester (test trên trình duyệt thật ở máy tính và điện thoại, **giữ lại test**, cập nhật `TEST-PLAN.md`). Bị trả lại quá N lần thì ticket chuyển sang Blocked. Mỗi ticket đạt thì **commit Git**. PM họp daily.
11. **Designer duyệt giao diện thật:** chụp màn hình sản phẩm, lỗi thì tạo ticket và quay lại bước Lập trình.
12. **Security:** chạy `npm audit`, dò bí mật bị lộ, soát theo OWASP Top 10, thử XSS trên trình duyệt; lỗi mức Cao trở lên chặn phát hành và trả về Dev; viết `SECURITY.md`.
13. **DevOps:** cài đặt, lint, typecheck, test, build (`dist/` với đường dẫn tương đối), mở bản xem thử, cập nhật README và CHANGELOG; lỗi thì report_issue và quay lại bước Lập trình.
14. **QA kiểm định cuối toàn bộ sản phẩm:** đi qua mọi tiêu chí nghiệm thu, viết `QA-REPORT.md`, lỗi thì quay lại bước Lập trình (tối đa 2 vòng).
15. **PM viết `SPECIFICATION.md`:** tài liệu đặc tả và hướng dẫn sử dụng, có **ảnh chụp thật** (`docs/images/`), sơ đồ luồng Mermaid, quy tắc nghiệp vụ, FAQ.
16. **CEO báo cáo** (`REPORT.md`), **PM họp rút kinh nghiệm** (`RETRO.md` và lưu bài học vào bộ nhớ chung), commit bàn giao, **lưu phiên bản .zip**, chờ Chủ tịch duyệt.
17. **Duyệt thì ra mắt:** Pháp chế, Marketing, Sales, CSKH, Kế toán, Nhân sự chuẩn bị song song, rồi sản phẩm bắt đầu **Vận hành**: cổng khách hàng 24/7; CSKH trả lời khách, lỗi thì chuyển Dev sửa rồi tự phát hành; Sales chốt đơn khi Chủ tịch xác nhận.
18. **Yêu cầu sửa:** quay lại bước 1 với ghi chú vòng sửa. Chỉ làm phần cần đổi.

## 7. Tự hồi phục khi lỗi

1. **Gọi model:** thử lại tối đa 6 lần khi gặp lỗi mạng, timeout, 429 hoặc 5xx. Chờ theo `Retry-After`, hoặc theo "reset after 1m 19s" nếu proxy báo trong nội dung lỗi. Lỗi liên tục thì chuyển sang endpoint dự phòng (`CUSTOM_FALLBACK_BASE_URL`).
2. **Cấp trên xử lý** (Tech Lead với kỹ sư, CEO với các vai khác, PM với CEO):
   - Lần 1: giao cho đồng nghiệp cùng vai, hoặc cho nhân viên đó chạy bằng **model dự phòng**.
   - Lần 2: **diễn đạt lại lời giao việc** (rõ ràng, chia bước nhỏ, trung tính, giữ nguyên mục tiêu và tiêu chí nghiệm thu) rồi giao lại.
3. **Dự án tự chạy lại** sau 5 phút, tối đa N lần; mọi diễn biến đều báo vào Chat.

## 8. Git, lịch sử, tải mã nguồn

- Mỗi dự án là một repo; thư mục của khách thì làm trên nhánh `ai-company/<slug>`. Commit sau mỗi bước và mỗi ticket, có commit bàn giao. Tự thêm `.gitignore`. **Không bao giờ tự push.**
- Mỗi lần bàn giao và mỗi lần sửa lỗi khi vận hành thì lưu phiên bản `data/history/<slug>/vN.zip` (bỏ node_modules, dist, .git…). API tải mã nguồn hiện tại và từng phiên bản.

## 9. Chi phí và chọn model

- Tính chi phí từ token × giá của từng nhân viên. Token suy nghĩ mà proxy báo riêng cũng tính vào đầu ra.
- **Model cho việc nhẹ** (bỏ phiếu, biên bản, họp daily) và **model dự phòng** đặt trong Cài đặt.
- Ngân sách theo dự án; vượt thì hỏi Chủ tịch.

## 10. Giao diện văn phòng (Canvas)

- Bố cục gồm các phòng: Chủ tịch, CEO, Họp, Pantry, Marketing, Sales, Sảnh và Nhân sự, Khu làm việc, Design, QA, CSKH, Kế toán, DevOps, Nhân sự và Pháp chế. Bàn ghế được xếp theo vai.
- Chuyển động: nhân viên đi theo đường (path), mang tài liệu khi bàn giao, vào phòng họp khi có sự kiện họp, đi uống cà phê, rời công ty khi nghỉ. Bong bóng hiện trạng thái (đang nghĩ, đang code, chờ, bị chặn). Vương miện cho người có KPI cao nhất.
- Điều khiển: lăn chuột để phóng to/thu nhỏ (listener native với `passive: false`); chụm hai ngón trên touchpad (sự kiện wheel kèm ctrlKey); nút + / − / vừa khung; kéo để di chuyển; nhấp vào nhân viên để mở ngăn chi tiết.
- Mọi cập nhật đẩy qua WebSocket: upsert, remove, usage, meeting, handoff, live activity.

## 11. Cài đặt

Tự cho phép mọi lệnh · số lượt gọi model đồng thời · số lần trả ticket tối đa · số bước mỗi lượt việc · ngân sách mặc định · số vòng thảo luận ở họp khởi động · model dự phòng · model cho việc nhẹ · số lần tự chạy lại · Chủ tịch duyệt thiết kế · độ trễ của Mock.

## 12. Chuẩn chất lượng bắt buộc

- Tiêu chuẩn "sản phẩm thương mại cấp production", không phải demo.
- Chia trang / module để nâng cấp từng phần; tệp dưới khoảng 300 dòng; không trùng lặp; mọi tài liệu viết tiếng Việt, code viết tiếng Anh.
- Mọi thay đổi trong ứng dụng phải: build sạch, có unit test cho logic quan trọng (đếm phiếu, đọc ballot, giải nén an toàn, phân quyền đường dẫn, lọc lệnh), và **chạy thử trọn quy trình bằng provider Mock** trên một server thử riêng (`PORT` và `AI_COMPANY_ROOT` khác) trước khi dùng thật.

## 13. Bảo mật của chính ứng dụng

Chỉ lắng nghe localhost (cổng khách hàng có thể mở riêng) · khóa API chỉ ở `.env` · nhân viên AI không ra được ngoài thư mục dự án · chặn lệnh nguy hiểm · chặn zip-slip và link Git dạng tham số · không nhận cả ổ đĩa hay thư mục của chính ứng dụng · chặn công cụ MCP chạy code tùy ý · không commit `.env`, `data/`, `workspace/`.

## 14. Muốn làm "công ty AI" ở lĩnh vực khác

Giữ nguyên lõi gồm Job, công cụ, MCP, skill, họp và bỏ phiếu, tự hồi phục, lịch sử phiên bản, văn phòng pixel. Chỉ thay:

1. **Danh sách vai và mô tả nghề** (`config.rs`, `skills/*.md`). Ví dụ công ty truyền thông: Biên tập, Nhà báo, Kiểm chứng thông tin, Thiết kế, SEO, Pháp lý.
2. **Các bước quy trình** (`workflow.rs`) và tài liệu mỗi bước tạo ra.
3. **Công cụ đặc thù** (`tools.rs`) và **MCP** hợp lĩnh vực.
4. **Tiêu chí nghiệm thu và kiểm định cuối** của sản phẩm đầu ra.
5. Các phòng trong văn phòng (`layout.ts`).

## 15. Thứ tự xây dựng gợi ý

1. Backend tối thiểu: types, store, API, WebSocket, provider Mock, một Job chạy được với công cụ đọc/ghi tệp.
2. Frontend: các bảng Dự án, Kanban, Chat, Nhân sự; văn phòng pixel tĩnh rồi động.
3. Quy trình cơ bản: Plan → Requirements → Build (Dev/Review/Test) → Release → Report → duyệt.
4. Provider thật (OpenAI-compatible, Anthropic), chi phí, ngân sách, duyệt lệnh.
5. Họp khởi động có thảo luận và bỏ phiếu; Designer; Tech Lead; Data Engineer.
6. MCP, skill, ảnh chụp màn hình cho model; Designer duyệt giao diện, Security, QA cuối, tài liệu đặc tả.
7. Constitution, hỏi làm rõ, mẫu đặc tả, kiểm tra nhất quán, khách duyệt thiết kế, rút kinh nghiệm.
8. Git, lịch sử .zip, nhận mã nguồn có sẵn, tự hồi phục.
9. Vận hành sau ra mắt, cổng khách hàng, Marketing / Sales / CSKH / Kế toán / Nhân sự / Pháp chế.
