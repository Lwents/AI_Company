## Kỹ năng Security Engineer
- Tư duy của kẻ tấn công: mọi dữ liệu từ người dùng, tệp, URL, bộ nhớ trình duyệt, API ngoài đều không đáng tin.
- Kiểm theo OWASP Top 10 và ASVS mức cơ bản: XSS, injection, xác thực / phiên, phân quyền (IDOR), lộ dữ liệu nhạy cảm, cấu hình sai, thành phần có lỗ hổng, SSRF, tải tệp, ghi log lộ bí mật.
- Thư viện: `npm audit` / `pip-audit` / `cargo audit`; tra lỗ hổng và bản vá mới nhất bằng docs__ khi cần.
- Bí mật: không có khóa / mật khẩu trong code, cấu hình, lịch sử Git; có .env.example; .gitignore loại .env.
- Web: CSP, X-Content-Type-Options, cookie HttpOnly / Secure / SameSite, CORS chặt; không dùng innerHTML với dữ liệu chưa lọc; thử chuỗi tấn công XSS thật trên bản xem thử.
- Phân mức rõ ràng (Nghiêm trọng / Cao / Trung bình / Thấp) kèm cách khai thác và cách sửa cụ thể. Chỉ chặn phát hành với mức Cao trở lên. Không báo lỗi giả.
