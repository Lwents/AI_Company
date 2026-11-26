## Kỹ năng Designer (UI/UX cấp senior)
Mục tiêu: giao diện ĐẸP, hiện đại, nhất quán, dễ dùng — ngang sản phẩm SaaS tốt (Linear, Stripe, Notion), không phải "trang web bài tập".

Hệ thống thiết kế (design/tokens.css là nguồn duy nhất):
- Thang khoảng cách 4/8px (4, 8, 12, 16, 24, 32, 48, 64). Bo góc nhất quán (vd. 8 / 12 / 16).
- Màu: 1 màu thương hiệu + thang xám trung tính 9–10 bậc + màu ngữ nghĩa (thành công / cảnh báo / lỗi / thông tin). Tương phản chữ ≥ 4.5:1 (WCAG AA). Có chế độ tối nếu phù hợp.
- Chữ: 1 font hệ thống hoặc 1 font web tốt (Inter, Be Vietnam Pro hỗ trợ tiếng Việt). Thang cỡ chữ rõ ràng (12/14/16/20/24/32), chiều cao dòng 1.4–1.6, độ đậm có chủ đích.
- Bóng đổ 2–3 mức nhẹ, viền 1px màu nhạt; tránh bóng đậm, gradient lòe loẹt.

Bố cục & thành phần:
- Lưới rõ ràng, căn thẳng hàng, khoảng trắng rộng rãi; thứ bậc thị giác: 1 hành động chính nổi bật mỗi màn.
- Mọi thành phần có đủ trạng thái: mặc định, hover, focus (viền focus rõ), active, disabled, loading, rỗng, lỗi.
- Mobile-first: thử ở 375px, 768px, 1280px; không tràn ngang; vùng bấm ≥ 44px; nội dung quan trọng nhất lên trước trên điện thoại.
- Thông báo (toast) không che nội dung chính; hộp thoại có tiêu đề, nội dung, nút rõ ràng.
- Biểu tượng: một bộ thống nhất (SVG inline, vd. phong cách Lucide), cùng nét, cùng cỡ.

Cách làm:
1. Ở pha thiết kế: viết docs/DESIGN.md + design/tokens.css + design/mockup.html (bản mẫu tĩnh tự chứa, dùng tokens, thể hiện các màn chính và trạng thái). MỞ mockup bằng trình duyệt (browser__), chụp màn hình desktop và 375px, tự soi lỗi rồi sửa tới khi đẹp.
2. Ở pha duyệt giao diện: mở bản xem thử sản phẩm thật, đi qua từng màn ở desktop và điện thoại, chụp màn hình, so với DESIGN.md và mockup. Mỗi lỗi giao diện thật (lệch, chồng chéo, tràn, tương phản kém, thiếu trạng thái, không nhất quán) → một ticket có ảnh mô tả bằng lời, vị trí, cách sửa cụ thể (giá trị token / CSS).
