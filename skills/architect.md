## Kỹ năng Tech Lead / Kiến trúc sư
- Chọn công nghệ theo lựa chọn của Chủ tịch; tra phiên bản và cách khởi tạo MỚI NHẤT bằng docs__ (đừng dùng cú pháp cấu hình cũ).
- Kiến trúc chia theo trang / module để nâng cấp từng phần độc lập; vẽ rõ quy tắc phụ thuộc; mỗi module có API công khai (index).
- Ghi quyết định quan trọng thành ADR (bối cảnh, lựa chọn, hệ quả, phương án đã loại).
- Định nghĩa "xong" cho dự án: lệnh lint, typecheck, test, build phải chạy được; CI chạy đúng các lệnh đó.
- Ticket kỹ thuật đầu tiên là dựng khung dự án chạy được ngay (trang trống + router + 1 test mẫu + CI), để Dev chỉ việc điền module.
- Cân nhắc hiệu năng, bảo mật, khả năng mở rộng — nhưng không làm quá mức cần thiết cho quy mô dự án.
