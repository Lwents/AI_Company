# AI Company

Công ty phần mềm do AI vận hành, hiển thị như một văn phòng 2D. Bạn là **Chủ tịch**: giao dự án, duyệt kết quả, tuyển nhân viên. Mỗi nhân viên là một AI agent thật, gắn với một model riêng (Claude, GPT, Gemini, model local…). Họ đi lại trong văn phòng, giao việc cho nhau và làm ra sản phẩm thật trong thư mục `workspace/`.

- **Backend:** Rust (axum + tokio), một file exe khoảng 4 MB.
- **Frontend:** React + Canvas 2D (Vite).

## Chạy

Yêu cầu: Rust (toolchain GNU + MinGW, giống dự án Xoay Proxy) và Node.js.

```powershell
cd D:\AI_Company
powershell -ExecutionPolicy Bypass -File .\build.ps1 -Run
```

Lệnh trên build giao diện và backend, sau đó mở `http://127.0.0.1:8787`. Những lần sau chỉ cần chạy `backend\target\release\ai-company.exe`.

**Khi sửa giao diện:** chạy backend như trên, rồi chạy `npm run dev` và mở `http://localhost:5173` (có hot reload).

## Dùng model thật

1. Sao chép `.env.example` thành `.env` rồi điền khóa của nhà cung cấp bạn dùng.
2. Khởi động lại backend.
3. Mở tab **Nhân sự**, chọn từng người rồi đổi Nhà cung cấp và Model.

| Nhà cung cấp | Biến trong `.env` | Ghi chú |
|---|---|---|
| Mock | — | Mặc định, miễn phí, chỉ diễn kịch bản mẫu |
| Anthropic | `ANTHROPIC_API_KEY` | Có sẵn giá của `claude-opus-5-5`, `claude-sonnet-5-5`, `claude-haiku-5-5`, `claude-fable-5-2` |
| OpenAI / OpenRouter / Gemini | `OPENAI_API_KEY` / `OPENROUTER_API_KEY` / `GEMINI_API_KEY` | Tự nhập tên model và giá |
| Ollama | `OLLAMA_BASE_URL` | Chạy trên máy, không cần khóa |
| Tùy chỉnh | `CUSTOM_API_KEY` + Base URL | Mọi endpoint tương thích OpenAI |

Gợi ý: CEO dùng Opus 5.5, BA và Dev dùng Sonnet 5.5, Tester dùng Haiku 5.5.

Với Claude, backend gửi `effort` theo cấu hình của từng nhân viên. Với Opus 5.5, Sonnet 5.5 và Fable 5.2, backend bật thêm **fallback phía server**: nếu model từ chối một yêu cầu vì chính sách, API tự chạy lại bằng model dự phòng. Nếu API không nhận tham số này, backend tự tắt nó.

## Nhân sự

Công ty mặc định có 18 người ở 4 khối, tất cả chạy Mock:

| Khối | Vai trò | Việc |
|---|---|---|
| Ban điều hành | CEO, Thư ký | Kế hoạch, tổng kết, báo cáo Chủ tịch; biên bản họp |
| Sản phẩm & Kỹ thuật | PM, BA, Designer, Tech Lead, Dev ×2, Reviewer, Tester, DevOps | Làm và bảo trì sản phẩm |
| Kinh doanh | Marketing, Sales, CSKH ×2 | Ra mắt, bán hàng, chăm sóc khách |
| Hành chính | Nhân sự, Kế toán, Pháp chế | KPI và tuyển dụng, thu – chi – lãi lỗ, điều khoản và bảo mật |

Vai trò nào chưa có người thì bước đó được bỏ qua. Bắt buộc phải có CEO và ít nhất 1 Dev.

## Vòng đời một sản phẩm

```
1. LÀM     Kế hoạch (CEO) → Yêu cầu (BA) → Sprint (PM) → Thiết kế (Designer) → Kiến trúc (Tech Lead)
           → Làm & test (Dev ⇄ Reviewer ⇄ Tester, họp daily) → Phát hành (DevOps) → Báo cáo (CEO)
2. DUYỆT   Chủ tịch bấm "Duyệt & ra mắt" (hoặc yêu cầu sửa)
3. RA MẮT  Các phòng ban làm song song: Pháp chế (điều khoản), Marketing (landing page, bài đăng, email, SEO),
           Sales (bảng giá, kịch bản), CSKH (FAQ, quy trình), Kế toán (báo cáo), Nhân sự (đánh giá)
4. VẬN HÀNH  (liên tục)
   • Khách vào Cổng khách hàng: hỏi, báo lỗi, hỏi mua → CSKH/Sales trả lời, khách xem phản hồi và chấm sao
   • Lỗi khách báo → ticket → Dev sửa → Reviewer → Tester → DevOps kiểm tra → CSKH báo lại khách
   • Sales chốt đơn → Chủ tịch xác nhận → ghi nhận doanh thu
   • Chủ tịch giao chiến dịch marketing, đòi báo cáo tài chính, đánh giá nhân sự bất cứ lúc nào (tab Vận hành)
   • Nhân sự đề xuất tuyển người → Chủ tịch duyệt → người mới tự vào công ty
```

### Cổng khách hàng

- Mỗi sản phẩm đang vận hành có trang `http://127.0.0.1:8787/portal/<mã dự án>/`. Trên mọi trang của sản phẩm cũng có nút "💬 Hỗ trợ".
- Muốn khách ở máy khác vào được, thêm `PORTAL_BIND=0.0.0.0:8790` vào `.env`. Địa chỉ này chỉ mở cổng khách hàng và sản phẩm; các thư mục nội bộ (`docs/`, `finance/`, `hr/`, `sales/`, `support/`…) và phần quản trị không lộ ra ngoài.
- Muốn đưa lên Internet, dùng một tunnel (ví dụ Cloudflare Tunnel) trỏ vào cổng 8790.
- Nút **Tạo 4 khách hàng thử** trong tab Vận hành giúp xem cả quy trình mà không cần khách thật.

## Văn phòng

- **14 phòng:** Phòng Chủ tịch (nhân vật Chủ tịch là bạn), CEO, Phòng họp, Pantry, Marketing, Sales, Sảnh (Lễ tân, Thư ký), Khu làm việc, Design, QA, CSKH (màn hình hàng chờ khách), Kế toán, DevOps & Server, Nhân sự & Pháp chế.
- **Hoạt cảnh:** khách liên hệ thì CSKH hiện "☎"; Sales chốt đơn thì cầm giấy đi báo Chủ tịch.
- **Ra vào:** người mới tuyển đi vào từ cửa chính ở sảnh; người nghỉ việc thì đi ra.
- **Tự mở rộng:** tuyển thêm người thì khu làm việc dài thêm. Phòng riêng hết chỗ thì người mới ngồi ở khu làm việc.
- **Camera:** lăn chuột để zoom, kéo để di chuyển, nhấp đúp để vừa màn hình. Bấm vào một người để xem họ đang làm gì.

## KPI

Tab **KPI** chấm điểm 0–100 cho từng người, xếp hạng S/A/B/C:

- **Kỹ thuật:** ticket đạt, bug bắt được, số lần review.
- **CSKH:** hồ sơ đã giải quyết, điểm hài lòng của khách.
- **Sales:** số đơn và doanh thu.
- **Văn phòng:** số đầu việc hoàn thành.
- Có gợi ý đổi model mạnh hơn hoặc rẻ hơn cho từng người. Người đứng đầu đội vương miện trong văn phòng.

## An toàn

- Server chỉ lắng nghe `127.0.0.1`, máy khác trong mạng không truy cập được.
- Nhân viên chỉ đọc/ghi được trong thư mục dự án của mình. CEO, BA và PM chỉ được ghi vào `docs/`; Tester chỉ được ghi vào `tests/` và `docs/`.
- Lệnh shell nằm trong danh sách an toàn (`node`, `npm test`, `pytest`, `cargo test`…) chạy luôn. Các lệnh khác phải chờ Chủ tịch duyệt ở tab **Duyệt**.
- Lệnh nguy hiểm (xóa đệ quy, tải từ mạng, ra ngoài thư mục dự án…) luôn bị chặn.
- API key bị gỡ khỏi biến môi trường trước khi chạy lệnh của agent.
- Mỗi dự án có ngân sách. Hết ngân sách, công ty dừng lại và hỏi Chủ tịch có cấp thêm không.

## Cấu trúc

```
backend/src/
  main.rs       khởi động server, phục vụ web/dist
  api.rs        REST + WebSocket
  workflow.rs   quy trình công ty (các phase, Dev ⇄ Tester)
  agent.rs      vòng lặp tool-use của một nhân viên
  tools.rs      tool + phân quyền + sandbox lệnh (có unit test)
  llm/          anthropic.rs (Messages API), openai.rs (OpenAI-compatible), mock.rs
  store.rs      trạng thái, lưu data/state.json, phát sự kiện
web/src/
  office/       văn phòng 2D (layout + mô phỏng di chuyển + vẽ)
  panels/       Dự án, Kanban, Chat, Nhân sự, Sản phẩm, Duyệt, Chi phí, Cài đặt
```

- Dữ liệu được lưu ở `data/state.json`.
- Sản phẩm của các dự án nằm ở `workspace/<tên-dự-án>/`.
- Hai thư mục này đã nằm trong `.gitignore`.

## Kiểm tra

```powershell
cd backend; cargo test        # unit test cho sandbox
npm run typecheck             # kiểm tra kiểu frontend
```

## Hướng phát triển

- Chạy lệnh trong Docker sandbox.
- Âm thanh.
- Lưu và tải nhiều "công ty" khác nhau.
