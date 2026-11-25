# AI Company — Ý tưởng dự án

> **Gửi AI thực hiện:** Đây là tài liệu ý tưởng. Hãy đọc hết, đặt câu hỏi nếu có chỗ chưa rõ, rồi đề xuất kiến trúc và làm **MVP (Giai đoạn 1)** trước. Không làm hết một lần. Trả lời bằng tiếng Việt.

---

## 1. Tóm tắt

Một ứng dụng mô phỏng **công ty phần mềm do AI vận hành**, hiển thị như game văn phòng 2D: các nhân viên (nhân vật nhỏ) đi lại trong văn phòng, ngồi bàn làm việc, đi họp, mang tài liệu cho nhau.

**Mỗi nhân viên là một AI agent thật**, gắn với một model AI (Claude, GPT, Gemini, model chạy local…). Nhiều nhân viên có thể dùng chung một model.

- **Người dùng = Chủ tịch / Hội đồng quản trị:** giao yêu cầu (ví dụ "Làm cho tôi web bán hàng"), duyệt kết quả, thuê/sa thải nhân viên.
- **CEO = model mạnh nhất:** nhận yêu cầu, chia việc, giao cho các phòng ban, tổng hợp báo cáo.
- **Nhân viên = model rẻ/nhanh hơn:** làm từng phần việc theo vai trò (BA, Dev, Tester…).

Mục tiêu: công ty **thực sự làm ra sản phẩm** (code, tài liệu, test), còn giao diện game giúp nhìn thấy trực quan ai đang làm gì.

---

## 2. Sơ đồ tổ chức

```
                Người dùng (Chủ tịch)
                        │
                       CEO  ← model mạnh nhất
          ┌─────────────┼──────────────┐
      PM / PO      Tech Lead /      Trưởng QA
          │         Architect           │
         BA      ┌──────┴──────┐     Tester(s)
                Dev FE     Dev BE
                Designer   DevOps
                        Reviewer
```

| Vai trò | Nhiệm vụ | Model gợi ý |
|---|---|---|
| **CEO** | Hiểu yêu cầu, lập kế hoạch tổng, chia việc, ra quyết định cuối, báo cáo cho Chủ tịch | Mạnh nhất (vd Claude Opus) |
| **PM / PO** | Tạo ticket, ưu tiên, theo dõi tiến độ, gọi họp | Trung bình |
| **BA** | Phân tích yêu cầu, viết user story + tiêu chí nghiệm thu | Trung bình |
| **Tech Lead / Architect** | Chọn công nghệ, thiết kế kiến trúc, chia task kỹ thuật | Mạnh (code giỏi) |
| **Dev FE / Dev BE** | Viết code theo ticket | Code giỏi, giá vừa (vd Claude Sonnet) |
| **Designer** | Thiết kế UI, màu sắc, bố cục | Trung bình / có khả năng xử lý ảnh |
| **Code Reviewer** | Review code, yêu cầu sửa | Mạnh |
| **Tester / QA** | Viết test case, chạy test, báo bug | Rẻ, nhanh (vd Claude Haiku) |
| **DevOps** | Build, chạy, đóng gói, triển khai | Rẻ, nhanh |
| **Thư ký / Docs** | Ghi biên bản họp, viết README, tóm tắt | Rẻ nhất |

**Bắt buộc:** vai trò, số lượng nhân viên và model của từng người **phải cấu hình được** trong giao diện, không hard-code.

---

## 3. Quy trình làm việc

1. **Chủ tịch** gửi yêu cầu → CEO nhận.
2. **CEO** hỏi lại nếu chưa rõ, sau đó viết **kế hoạch** và giao cho PM.
3. **BA** viết user story + tiêu chí nghiệm thu → PM tạo ticket trên bảng Kanban.
4. **Tech Lead** thiết kế kiến trúc, chia ticket kỹ thuật cho Dev.
5. **Dev** nhận ticket → viết code trong workspace riêng (git branch) → gửi review.
6. **Reviewer** duyệt hoặc trả lại kèm góp ý (tối đa N vòng).
7. **Tester** chạy test, viết bug ticket nếu lỗi → quay về Dev.
8. **DevOps** build và chạy thử sản phẩm.
9. **CEO** tổng hợp → báo cáo Chủ tịch → Chủ tịch **duyệt / yêu cầu sửa**.

Trạng thái ticket: `Backlog → To Do → In Progress → Review → Testing → Done` (và `Blocked`).

**Họp:** PM có thể gọi họp (daily standup, họp xử lý sự cố). Các nhân viên đi vào phòng họp, mỗi người nói một lượt, thư ký ghi biên bản.

---

## 4. Giao diện game

### Văn phòng (màn hình chính)
- Bản đồ 2D nhìn từ trên xuống, phong cách pixel art hoặc flat, gồm: phòng CEO, khu bàn Dev, khu QA, phòng họp, máy pha cà phê, bảng Kanban treo tường.
- Mỗi nhân viên là một nhân vật nhỏ, có tên + nhãn model (vd "Lan · BA · Sonnet").
- **Hành vi trực quan gắn với việc thật:**
  - Đang gọi API model → ngồi gõ phím, có bong bóng "…" hoặc "đang suy nghĩ".
  - Gửi tin / bàn giao việc cho người khác → **đi bộ tới bàn người đó**, cầm tập giấy.
  - Họp → đi vào phòng họp.
  - Rảnh → đi lấy cà phê, đi dạo.
  - Bị chặn / lỗi → biểu tượng ❗ trên đầu.
- Bấm vào nhân viên → mở panel: việc đang làm, lịch sử hội thoại, chi phí token, đổi model, đổi system prompt.

### Các màn hình khác
- **Bảng Kanban:** ticket, ai phụ trách, trạng thái.
- **Kênh chat nội bộ:** xem toàn bộ tin nhắn giữa các agent (như Slack).
- **Tài chính:** tiền token đã tiêu theo từng nhân viên / dự án (coi như "lương").
- **Nhân sự:** thuê / sa thải / đổi model / đổi vai trò.
- **Sản phẩm:** cây thư mục code, tài liệu đã tạo, nút chạy thử.

---

## 5. Cơ chế "công ty"

- **Ngân sách:** mỗi dự án có hạn mức chi phí. Hết tiền thì công ty dừng và hỏi Chủ tịch.
- **KPI nhân viên:** số ticket hoàn thành, số lần bị trả về, số bug gây ra, chi phí. Gợi ý cho Chủ tịch đổi model nếu nhân viên làm kém.
- **Thăng chức / thay model:** đổi model của một vị trí ngay khi đang chạy.
- **Chạy song song:** nhiều nhân viên làm cùng lúc, có giới hạn số lần gọi API đồng thời.
- **Phê duyệt của Chủ tịch:** bắt buộc với các việc quan trọng (xóa file, chạy lệnh nguy hiểm, deploy, vượt ngân sách).
- **Tốc độ game:** tạm dừng / chạy 1x / 2x (chỉ ảnh hưởng hoạt ảnh, không ảnh hưởng tốc độ AI).

---

## 6. Kỹ thuật đề xuất

| Phần | Gợi ý |
|---|---|
| Frontend | React + TypeScript; bản đồ văn phòng bằng **PixiJS** hoặc **Phaser 3** |
| Backend | Node.js (TypeScript) hoặc Python (FastAPI) |
| Gọi model | Một lớp trung gian chung (vd **LiteLLM** hoặc **OpenRouter**) để hỗ trợ Anthropic, OpenAI, Google, Ollama local |
| Real-time | WebSocket đẩy sự kiện từ backend → animation trên frontend |
| Lưu trữ | SQLite (MVP), sau này Postgres |
| Workspace code | Mỗi dự án một thư mục git; chạy code/test trong **Docker sandbox** |
| Đóng gói (tùy chọn) | Chạy trên web trước; sau có thể đóng gói desktop bằng Tauri/Electron |

### Thiết kế cốt lõi
- **Agent** = `{ id, tên, vai trò, model, system_prompt, tools[], ngân sách, trạng thái }`.
- **Trạng thái agent:** `idle | thinking | walking | working | in_meeting | waiting | blocked`.
- **Event bus:** mọi hành động sinh ra sự kiện (`task_assigned`, `message_sent`, `meeting_started`, `tool_called`, `cost_updated`…). Frontend chỉ nghe sự kiện để vẽ animation → **logic AI và giao diện game tách rời hoàn toàn**.
- **Tool cho agent** (theo quyền vai trò): đọc/ghi file, chạy lệnh trong sandbox, tạo/cập nhật ticket, nhắn tin cho đồng nghiệp, gọi họp, hỏi Chủ tịch.
- **Bộ nhớ:** mỗi agent có lịch sử riêng + bộ nhớ chung của dự án (tài liệu, quyết định). Tóm tắt khi hội thoại quá dài.
- **API key:** lưu trong `.env` / kho bảo mật, không ghi vào code hay log.

---

## 7. Lộ trình

**Giai đoạn 1 — MVP (làm trước):**
- 4 nhân viên cố định: CEO, BA, Dev, Tester; mỗi người chọn được model.
- Quy trình: yêu cầu → kế hoạch → code → test → báo cáo.
- Văn phòng 2D đơn giản: nhân vật đi lại giữa các bàn theo sự kiện thật, có bong bóng trạng thái.
- Kanban + kênh chat nội bộ + theo dõi chi phí.
- Sản phẩm đầu ra: một project nhỏ chạy được (vd trang todo-list).

**Giai đoạn 2:** đủ các vai trò, họp, review nhiều vòng, Docker sandbox, ngân sách, phê duyệt.

**Giai đoạn 3:** thuê/sa thải, KPI, lưu/tải công ty, nhiều dự án song song, đồ họa đẹp hơn, âm thanh.

---

## 8. Tiêu chí hoàn thành MVP

- [ ] Nhập 1 yêu cầu → công ty tự làm ra project chạy được mà không cần can thiệp (trừ các bước phê duyệt).
- [ ] Mỗi lần agent gọi model hoặc bàn giao việc đều có animation tương ứng trên bản đồ.
- [ ] Đổi model của một nhân viên trong giao diện là có hiệu lực ngay.
- [ ] Xem được toàn bộ tin nhắn giữa các agent và tổng chi phí.
- [ ] Không lộ API key trong code / log.

---

## 9. Yêu cầu với AI thực hiện

1. Đề xuất cấu trúc thư mục + kiến trúc chi tiết trước khi code.
2. Làm theo từng giai đoạn, mỗi giai đoạn chạy được và có hướng dẫn chạy.
3. Có chế độ **mock model** (không tốn tiền) để test giao diện và luồng.
4. Viết README hướng dẫn cài đặt, cấu hình API key, thêm nhân viên mới.
5. Thiết kế nhân vật và văn phòng **tự tạo**, không sao chép hình ảnh hay nhân vật từ game có sẵn.
