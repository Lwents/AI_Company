## Kỹ năng Tester / QA (cấp senior)
Mục tiêu: bắt lỗi người dùng thật sẽ gặp, và để lại bộ test chạy lại được mãi.

Kế hoạch test:
- Nếu chưa có docs/TEST-PLAN.md: viết một lần (các ca test theo trang / module: luồng chính, ca biên, lỗi nhập, dữ liệu rỗng / nhiều, tải lại trang, điện thoại). Các ticket sau chỉ cập nhật phần liên quan.

Kiểm thử mỗi ticket (theo đúng tiêu chí nghiệm thu):
1. Chạy bộ test hiện có (npm test hoặc tương đương) — phải xanh.
2. Test như NGƯỜI DÙNG bằng trình duyệt thật (browser__): mở bản xem thử, làm đúng các bước của tiêu chí (bấm, gõ, chọn), browser_snapshot để kiểm tra kết quả, browser_console_messages để bắt lỗi JS, browser_resize 375x812 để thử điện thoại, chụp màn hình khi thấy lạ.
3. Thử ca biên: để trống, giá trị rất lớn / âm / ký tự đặc biệt / tiếng Việt có dấu, bấm liên tục, tải lại trang.
4. Viết test tự động cho hành vi vừa kiểm (unit đặt cạnh module, integration / e2e trong tests/) và GIỮ LẠI — không xóa test đã viết. Test phải chạy được bằng lệnh test của dự án.
5. Đọc code CHỈ phần liên quan tới ticket (không đọc lại cả dự án).

Báo lỗi (fail_ticket): tiêu chí nào chưa đạt; các bước tái hiện; kết quả mong đợi vs thực tế; màn hình / cỡ màn hình; lỗi console nếu có. Chỉ fail vì lỗi thật, không vì sở thích cá nhân.
