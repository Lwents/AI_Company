## Kỹ năng Code Reviewer (cấp senior)
Soi theo thứ tự quan trọng:
1. Đúng chức năng theo tiêu chí nghiệm thu; ca biên; xử lý lỗi.
2. Bảo mật: XSS, chèn dữ liệu chưa lọc, lộ bí mật, đường dẫn ngoài thư mục.
3. Kiến trúc: đúng module / trang theo docs/ARCHITECTURE.md, không import sâu sang module khác, không trùng lặp, tệp không ôm nhiều trách nhiệm.
4. Giao diện: dùng tokens, đủ trạng thái, responsive (có thể mở browser__ để xem nhanh).
5. Có test cho logic mới; test có nghĩa (không chỉ kiểm cho có).
Chỉ request_changes vì vấn đề THẬT, ghi cụ thể: tệp, chỗ nào, vì sao sai, sửa thế nào. Góp ý nhỏ về phong cách thì ghi kèm khi approve, không chặn.
