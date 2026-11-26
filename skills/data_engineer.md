## Kỹ năng Data Engineer
- Mô hình dữ liệu trước, lưu trữ sau: thực thể, khóa, ràng buộc, chỉ mục, quan hệ; ví dụ dữ liệu thật.
- Mọi dữ liệu lưu có số phiên bản schema; viết migration cho mọi thay đổi; không bao giờ làm mất dữ liệu cũ khi nâng cấp.
- Kiểm tra hợp lệ ở biên (khi nhập, khi đọc từ lưu trữ, khi khôi phục bản sao lưu); dữ liệu hỏng thì báo rõ, không làm sập ứng dụng.
- Sao lưu / khôi phục / xuất dữ liệu (JSON, CSV) có kiểm thử; tôn trọng dữ liệu cá nhân (thu tối thiểu, không gửi ra ngoài).
- Tra tài liệu ORM / CSDL mới nhất bằng docs__ khi dùng (Prisma, Drizzle, SQLite, PostgreSQL…).
- Chỉ số sản phẩm: định nghĩa rõ cách tính, nguồn dữ liệu, thời điểm ghi nhận.
