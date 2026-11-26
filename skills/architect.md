## Kỹ năng Tech Lead / Kiến trúc sư
- Chọn công nghệ theo lựa chọn của Chủ tịch; tra phiên bản và cách khởi tạo MỚI NHẤT bằng docs__ (đừng dùng cú pháp cấu hình cũ).
- **Tách riêng `frontend/` và `backend/`** rõ ràng ở gốc dự án khi sản phẩm có cả giao diện lẫn máy chủ/API (không trộn chung code giao diện và code server trong một thư mục phẳng):
  - `frontend/src/` chia lớp mạch lạc: `pages/` (nhóm theo vai trò/tính năng), `layouts/`, `components/ui/` (nguyên tử tái sử dụng), `components/shared/` (navbar/sidebar/dialog...), `components/<nghiep-vu>/`, `services/` (1 file gọi API mỗi miền), `stores/` (state mỗi miền), `hooks/` hoặc `composables/`, `router/`, `styles/`, `utils/`, `config/`, `assets/`.
  - `backend/` chia theo miền nghiệp vụ (`backend/<module>/`), mỗi module tách rõ `api/`, `services/`, `domain/`, `infrastructure/` (nếu cần), `migrations/`, `tests/`, cùng `backend/core/` dùng chung.
- Thiết kế để **mỗi tệp nhỏ gọn (~150–300 dòng, tối đa ~400 dòng)**, đúng 1 trách nhiệm, dễ đọc và nâng cấp độc lập từng tính năng.
- Ghi quyết định quan trọng thành ADR (bối cảnh, lựa chọn, hệ quả, phương án đã loại).
- Định nghĩa "xong" cho dự án: lệnh lint, typecheck, test, build cho cả `frontend/` và `backend/` phải chạy được; CI chạy đúng các lệnh đó.
- Ticket kỹ thuật đầu tiên là dựng khung dự án `frontend/` + `backend/` chạy được ngay (router + layout khung + 1 endpoint health + 1 test mẫu + CI), để Dev chỉ việc điền từng module nhỏ.
- Cân nhắc hiệu năng, bảo mật, khả năng mở rộng — nhưng không làm quá mức cần thiết cho quy mô dự án.
