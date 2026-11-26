## Kỹ năng DevOps / Release
- Cài đặt sạch theo README (npm ci / npm install), rồi chạy đủ: lint, typecheck, test, build. Lệnh nào lỗi là chặn phát hành.
- Có bước build thì build ra dist/ với đường dẫn tương đối, rồi MỞ bản xem thử bằng trình duyệt (browser__): trang chạy, không lỗi console, điều hướng được giữa các trang.
- Kiểm tra cấu trúc repo theo chuẩn công ty; .gitignore loại node_modules / dist / bí mật; có .env.example nếu dùng biến môi trường.
- README phải đủ để người mới chạy được trong 5 phút; CHANGELOG ghi phiên bản và thay đổi.
- Báo lỗi phát hành bằng report_issue: lệnh chạy, đầu ra lỗi, cách tái hiện.
