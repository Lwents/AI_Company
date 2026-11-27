export function GuidePanel() {
  return (
    <div className="panel-body">
      <section className="card">
        <h3>1. Giao dự án &amp; Mã nguồn (Tab Dự án)</h3>
        <ul className="plain small" style={{ display: "grid", gap: 8 }}>
          <li>
            <b>Làm mới:</b> PM tư vấn và hỏi bạn chọn công nghệ bằng ngôn ngữ dễ hiểu. Khi hoàn thành sẽ bàn giao kèm tài liệu đặc tả và hướng dẫn sử dụng có ảnh chụp.
          </li>
          <li>
            <b>Tải .zip / Link Git:</b> Đội đọc hiểu mã nguồn trước, giữ nguyên công nghệ và cấu trúc sẵn có, rồi sửa hoặc nâng cấp đúng theo yêu cầu.
          </li>
          <li>
            <b>Thư mục máy:</b> Đội sửa <b>trực tiếp</b> trong thư mục bạn chọn. Nếu có Git thì xem/hoàn tác bằng Git; nếu chưa có Git, bản gốc được tự lưu thành <code>v1 (.zip)</code> trong Lịch sử.
          </li>
        </ul>
      </section>

      <section className="card">
        <h3>2. Nhân sự &amp; Skill (Tab Nhân sự)</h3>
        <ul className="plain small" style={{ display: "grid", gap: 8 }}>
          <li>
            <b>Vai trò &amp; Mock:</b> Mọi vai trò đều có việc trong quy trình; vai trò nào chưa có người sẽ tự bỏ qua. Nhân viên mới mặc định chạy <b>Mock</b> (miễn phí), đổi sang model thật ngay trong thẻ nhân viên.
          </li>
          <li>
            <b>Skill mặc định:</b> Tự động nạp <code>skills/common.md</code>, <code>skills/&lt;vai_tro&gt;.md</code> và thư viện skill theo bước trong <code>skills/library.json</code>.
          </li>
          <li>
            <b>Skill riêng:</b> Dán nội dung Markdown, kéo thả tệp <code>.md/.txt</code> hoặc trỏ đường dẫn tệp (tự đọc bản mới nhất mỗi lần chạy) để bổ sung cùng skill mặc định của vai trò.
          </li>
        </ul>
      </section>

      <section className="card">
        <h3>3. Vận hành &amp; Cổng khách hàng (Tab Vận hành)</h3>
        <ul className="plain small" style={{ display: "grid", gap: 8 }}>
          <li>
            <b>Ra mắt sản phẩm:</b> Bắt đầu sau khi Chủ tịch bấm <b>Duyệt &amp; ra mắt</b> ở tab <b>Dự án</b>. Khi đó Marketing, Sales, CSKH, Pháp chế, Kế toán và Nhân sự cùng vào cuộc.
          </li>
          <li>
            <b>Mở cổng cho máy khác truy cập:</b> Đặt <code>PORTAL_BIND=0.0.0.0:8790</code> trong file <code>.env</code> (chỉ mở cổng khách hàng và sản phẩm, không mở trang quản trị).
          </li>
        </ul>
      </section>

      <section className="card">
        <h3>4. Cài đặt vận hành, MCP &amp; Khóa API (Tab Cài đặt)</h3>
        <ul className="plain small" style={{ display: "grid", gap: 8 }}>
          <li>
            <b>Tự cho phép mọi lệnh:</b> Chỉ bật khi bạn tin tưởng model vì nhân viên có thể chạy lệnh tùy ý trong thư mục dự án.
          </li>
          <li>
            <b>Số vòng thảo luận họp khởi động:</b> Trưởng bộ phận góp ý rồi phản biện bản nháp của CEO trước khi chốt. Đặt <code>0</code> để CEO tự quyết (mỗi vòng tốn thêm khoảng 12 lượt gọi model).
          </li>
          <li>
            <b>Chế độ làm tools:</b> Tắt mặc định để tiết kiệm token. Khi bật, toàn bộ nhân viên sẽ nạp thêm tệp bạn đã tải lên ở mục này.
          </li>
          <li>
            <b>Tự chạy lại &amp; Model dự phòng:</b> Khi một nhân viên gặp lỗi hoặc bị model từ chối, cấp trên giao lại cho đồng nghiệp cùng vai hoặc chạy bằng model dự phòng, rồi diễn đạt lại yêu cầu rõ hơn nếu vẫn lỗi.
          </li>
          <li>
            <b>Công cụ MCP:</b> Bật/tắt áp dụng từ lượt việc tiếp theo, không cần khởi động lại:
            <div className="muted" style={{ marginTop: 4, display: "grid", gap: 3 }}>
              <span>• <code>browser</code> — Trình duyệt: mở bản xem thử, bấm, gõ, chụp màn hình và đọc lỗi như người dùng thật.</span>
              <span>• <code>desktop</code> — Điều khiển máy tính: thao tác ứng dụng Windows / giả lập để test thật (chỉ chạy trên Windows).</span>
              <span>• <code>docs</code> — Tài liệu thư viện: tra tài liệu mới nhất trước khi viết code.</span>
              <span>• <code>memory</code> — Bộ nhớ công ty: lưu và tra bài học kinh nghiệm qua các dự án.</span>
              <span>• <code>thinking</code> — Suy nghĩ từng bước: phân tích bài toán khó (tốn thêm lượt gọi model).</span>
            </div>
          </li>
          <li>
            <b>Khóa API:</b> Đặt trong file <code>.env</code> ở thư mục gốc dự án (tham khảo <code>.env.example</code>), không lưu trên giao diện. Sau khi sửa <code>.env</code> cần khởi động lại backend.
          </li>
        </ul>
      </section>

      <section className="card">
        <h3>5. KPI &amp; Chi phí (Tab KPI / Chi phí)</h3>
        <ul className="plain small" style={{ display: "grid", gap: 8 }}>
          <li>
            <b>Điểm KPI (0–100):</b> Đánh giá theo độ ổn định (tỉ lệ việc không lỗi), khối lượng hoàn thành theo vai trò và tỉ lệ ticket đạt ngay không bị trả lại (với Dev).
          </li>
          <li>
            <b>Chi phí AI:</b> Ước tính theo giá input/output ($/1M token) bạn đặt cho từng nhân viên; nhân viên chạy <b>Mock</b> tính <code>$0</code>.
          </li>
        </ul>
      </section>
    </div>
  );
}
