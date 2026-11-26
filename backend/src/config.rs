use crate::types::{Employee, ProviderId, Role};

pub fn role_label(r: Role) -> &'static str {
    match r {
        Role::Ceo => "CEO",
        Role::Pm => "PM",
        Role::Ba => "BA",
        Role::Architect => "Tech Lead",
        Role::Dev => "Dev",
        Role::Reviewer => "Reviewer",
        Role::Tester => "Tester",
        Role::Devops => "DevOps",
        Role::Designer => "Designer",
        Role::Secretary => "Thư ký",
        Role::Marketing => "Marketing",
        Role::Sales => "Sales",
        Role::Support => "CSKH",
        Role::Hr => "Nhân sự",
        Role::Accountant => "Kế toán",
        Role::DataEngineer => "Data Engineer",
        Role::Security => "Security",
        Role::Legal => "Pháp chế",
    }
}

pub fn role_duty(r: Role) -> &'static str {
    match r {
        Role::Ceo => "Bạn là CEO. Bạn hiểu yêu cầu của Chủ tịch, lập kế hoạch tổng, ra quyết định và báo cáo kết quả cuối cùng. Bạn không tự viết code sản phẩm.",
        Role::Pm => "Bạn là Project Manager. Bạn sắp xếp độ ưu tiên, lập kế hoạch sprint, chủ trì họp daily và gỡ vướng mắc cho đội.",
        Role::Ba => "Bạn là Business Analyst. Bạn biến kế hoạch thành yêu cầu rõ ràng, user story và tiêu chí nghiệm thu kiểm tra được.",
        Role::Architect => "Bạn là Tech Lead. Bạn chọn công nghệ hiện đại, phổ biến, được bảo trì tốt (theo lựa chọn của Chủ tịch nếu có), thiết kế kiến trúc chia module / trang để từng phần nâng cấp độc lập, định ra cấu trúc thư mục và quy ước code.",
        Role::Dev => "Bạn là lập trình viên. Bạn viết code chạy được, gọn, đúng yêu cầu của ticket và tự kiểm tra trước khi bàn giao.",
        Role::Reviewer => "Bạn là người review code. Bạn tìm lỗi thật (sai logic, thiếu yêu cầu, lỗ hổng bảo mật), không bắt bẻ chuyện vặt.",
        Role::Tester => "Bạn là Tester/QA. Bạn kiểm tra sản phẩm theo tiêu chí nghiệm thu bằng cách đọc code và chạy lệnh; báo lỗi cụ thể, tái hiện được.",
        Role::Devops => "Bạn là DevOps. Bạn kiểm tra sản phẩm build và chạy được, viết hướng dẫn cài đặt/chạy, và chặn phát hành khi có lỗi nghiêm trọng.",
        Role::Designer => "Bạn là UI/UX Designer. Bạn thiết kế giao diện dễ dùng: bảng màu, font, bố cục, trạng thái, và xuất design token cho Dev.",
        Role::Secretary => "Bạn là thư ký công ty. Bạn ghi biên bản họp chính xác, ngắn gọn, có quyết định và phân công rõ ràng.",
        Role::Marketing => "Bạn là chuyên viên Marketing. Bạn hiểu sản phẩm, xác định khách hàng mục tiêu và viết nội dung hấp dẫn, trung thực (landing page, bài mạng xã hội, email, SEO). Không hứa điều sản phẩm không làm được.",
        Role::Sales => "Bạn là nhân viên Sales. Bạn tư vấn tận tình, hỏi đúng nhu cầu, giới thiệu gói phù hợp theo bảng giá, và chỉ đề xuất chốt đơn khi khách đồng ý rõ ràng.",
        Role::Support => "Bạn là nhân viên Chăm sóc khách hàng. Bạn trả lời lịch sự, ngắn gọn, đúng sự thật dựa trên tài liệu sản phẩm; lỗi thật thì chuyển đội kỹ thuật và hẹn báo lại; không bịa thông tin.",
        Role::Hr => "Bạn là chuyên viên Nhân sự. Bạn đánh giá hiệu suất nhân viên dựa trên số liệu KPI, đề xuất đào tạo, đổi model hoặc tuyển thêm khi khối lượng việc vượt sức.",
        Role::Security => "Bạn là Kỹ sư bảo mật (Security Engineer). Bạn tìm lỗ hổng trong code và thư viện, bí mật bị lộ, cấu hình không an toàn, kiểm theo OWASP Top 10, đề xuất cách sửa cụ thể và có quyền chặn phát hành khi có lỗi nghiêm trọng.",
        Role::DataEngineer => "Bạn là Data Engineer. Bạn thiết kế mô hình dữ liệu (thực thể, trường, kiểu, ràng buộc, quan hệ), cách lưu trữ, kiểm tra hợp lệ dữ liệu, phiên bản schema và chuyển đổi dữ liệu khi nâng cấp, sao lưu / khôi phục, bảo vệ dữ liệu cá nhân, và các chỉ số cần đo. Bạn ưu tiên giải pháp đơn giản nhất mà không làm mất dữ liệu.",
        Role::Accountant => "Bạn là Kế toán. Bạn lập báo cáo thu – chi – lãi lỗ chính xác từ số liệu thật của công ty, chỉ ra khoản tốn kém và đề xuất tiết kiệm.",
        Role::Legal => "Bạn là chuyên viên Pháp chế. Bạn soạn điều khoản sử dụng, chính sách bảo mật dễ hiểu, phù hợp pháp luật Việt Nam, và cảnh báo rủi ro pháp lý trong nội dung quảng cáo.",
    }
}

/// The company's engineering standard for every product repository.
pub const REPO_STANDARD: &str = "CHUẨN CẤU TRÚC DỰ ÁN CỦA CÔNG TY (bắt buộc — sạch, tách tầng rõ ràng như dự án sản xuất chuyên nghiệp). Mục tiêu số 1: NGƯỜI ĐỌC MỞ RA HIỂU NGAY VÀ DỄ NÂNG CẤP TỪNG PHẦN NHỎ (không nhồi code vào 1–2 tệp khổng lồ).
- TÁCH RIÊNG FRONTEND & BACKEND ở gốc dự án khi sản phẩm có cả giao diện lẫn máy chủ/API (bất kể dùng React, Vue, Next, FastAPI, Django, Node/Express, Rust, Go...):
  1. `frontend/` (giao diện người dùng — tách lớp rõ ràng):
     - `frontend/public/`: tài nguyên tĩnh (favicon, ảnh công khai).
     - `frontend/src/pages/<nhom-hoac-vai-tro>/<ten-trang>/`: từng trang riêng biệt (vd. `pages/auth/Login`, `pages/admin/users/Users`, `pages/common/NotFound`). Trang chỉ lắp ghép component & gọi store/service, KHÔNG nhét logic xử lý dài.
     - `frontend/src/layouts/`: khung giao diện chung (AdminLayout, MainLayout, AuthLayout...).
     - `frontend/src/components/ui/`: component nguyên tử tái sử dụng (Button, Card, Input, Modal, Table, Badge, Select, Pagination...).
     - `frontend/src/components/shared/`: thành phần dùng chung toàn app (Navbar, Sidebar, Footer, Breadcrumb, ConfirmDialog, NotificationBell...).
     - `frontend/src/components/<nghiep-vu>/`: widget chuyên biệt của từng tính năng.
     - `frontend/src/services/`: mỗi miền nghiệp vụ 1 tệp gọi API riêng (`auth.service.*`, `user.service.*`...).
     - `frontend/src/stores/`: quản lý state theo từng miền (`auth.store.*`, `ui.store.*`, `theme.store.*`...).
     - `frontend/src/hooks/` (React) hoặc `frontend/src/composables/` (Vue): logic dùng lại.
     - `frontend/src/router/`: đăng ký định tuyến tập trung; `frontend/src/styles/`: design tokens (`tokens.css`) và style chung; `frontend/src/utils/`: `formatters`, `validators`, `constants`, `toast`; `frontend/src/config/` & `frontend/src/assets/`.
  2. `backend/` (máy chủ / API / nghiệp vụ — chia theo miền nghiệp vụ):
     - `backend/<ten-module>/api/` (routes / controllers / views / serializers — chỉ nhận/trả request).
     - `backend/<ten-module>/services/` (luồng nghiệp vụ / use cases).
     - `backend/<ten-module>/domain/` (model, thực thể, luật nghiệp vụ thuần).
     - `backend/<ten-module>/infrastructure/` (DB, hàng đợi, adapter ngoài nếu cần).
     - `backend/<ten-module>/migrations/` & `backend/<ten-module>/tests/`.
     - `backend/core/` (hoặc `backend/shared/`): cấu hình, middleware, auth, lỗi chung.
  (Nếu dự án thuần 1 đầu như web tĩnh không backend hoặc công cụ CLI thuần không giao diện, giữ nguyên cách tách lớp nhỏ gọn tương ứng; nếu nâng cấp repo có sẵn, tôn trọng điểm vào cũ nhưng vẫn tách module nhỏ gọn rõ ràng.)
- Gốc dự án gọn gàng: `README.md` (mô tả sản phẩm, sơ đồ cây thư mục `frontend/` & `backend/`, yêu cầu, cách cài, chạy dev, test, build), `CHANGELOG.md`, `.gitignore`, `.editorconfig`, `.env.example`, `docs/`, `tests/` (integration/e2e xuyên tầng), `.github/workflows/ci.yml`.
- `docs/`: PLAN, DECISIONS, REQUIREMENTS, DESIGN, ARCHITECTURE (có sơ đồ cây thư mục + bảng module/trang), DATA, RELEASE, REPORT; `docs/adr/`; `docs/modules/<ten-module>.md`.
- CHỐNG TỆP KHỔNG LỒ (bắt buộc): mỗi tệp đúng 1 trách nhiệm, giữ gọn **~150–300 dòng** (tuyệt đối tránh tệp > 400 dòng ôm nhiều chức năng — hãy tách nhỏ thành component / service / domain module con); không trùng lặp, không code chết; quy tắc phụ thuộc 1 chiều (`pages → components/stores/services → utils`, backend `api → services → domain`), không vòng phụ thuộc. Nếu frontend có bước build (Vite), build ra `frontend/dist/` (hoặc `dist/`) với `base: './'` để mở xem thử được ngay.";

/// The company's default technology direction when the chairman has no preference.
pub const TECH_GUIDE: &str = "Công nghệ hiện đại, phổ biến, bảo trì tốt và phân tách rõ Frontend / Backend (ví dụ Frontend: Vite + TypeScript + React hoặc Vue 3 + Tailwind/CSS tokens; Backend: FastAPI/Django với Python, hoặc Fastify/NestJS/Express + TypeScript với Node.js, hoặc Axum với Rust; Dữ liệu: SQLite/PostgreSQL; Test: Vitest/Pytest/Playwright). Chọn đúng công nghệ Chủ tịch yêu cầu nếu có, luôn chia nhỏ module dễ đọc và dễ nâng cấp.";

/// Where employees (and their browser) can open a project's preview.
pub fn preview_url(project_id: &str) -> String {
    let port = std::env::var("PORT").ok().filter(|p| !p.trim().is_empty()).unwrap_or_else(|| "8787".into());
    format!("http://127.0.0.1:{port}/preview/{project_id}/")
}

/// USD per 1M tokens (Anthropic first-party rates). Other providers: enter prices in the Staff panel.
pub fn known_price(model: &str) -> (f64, f64) {
    match model {
        "claude-fable-5-2" => (8.0, 40.0),
        "claude-opus-5-5" => (4.0, 20.0),
        "claude-sonnet-5-5" => (2.0, 10.0),
        "claude-haiku-5-5" => (0.10, 0.50),
        // Gemini 4 family (also via proxies such as 9router "ag/…"): Google list prices.
        m if m.contains("gemini-4-neon") => (3.0, 15.0),
        m if m.contains("gemini-4-carbon") || m.contains("gemini-4-argon") => (2.0, 10.0),
        m if m.contains("gemini-4-hydrogen") => (1.5, 7.5),
        _ => (0.0, 0.0),
    }
}

pub fn provider_env_key(p: ProviderId) -> Option<&'static str> {
    match p {
        ProviderId::Anthropic => Some("ANTHROPIC_API_KEY"),
        ProviderId::Openai => Some("OPENAI_API_KEY"),
        ProviderId::Openrouter => Some("OPENROUTER_API_KEY"),
        ProviderId::Gemini => Some("GEMINI_API_KEY"),
        ProviderId::Custom => Some("CUSTOM_API_KEY"),
        ProviderId::Mock | ProviderId::Ollama => None,
    }
}

pub fn provider_base_url(p: ProviderId) -> Option<String> {
    match p {
        ProviderId::Openai => Some("https://api.openai.com/v1".into()),
        ProviderId::Openrouter => Some("https://openrouter.ai/api/v1".into()),
        ProviderId::Gemini => Some("https://generativelanguage.googleapis.com/v1beta/openai".into()),
        ProviderId::Ollama => Some(std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434/v1".into())),
        _ => None,
    }
}

fn env_set(k: &str) -> bool {
    std::env::var(k).map(|v| !v.trim().is_empty()).unwrap_or(false)
}

pub fn provider_ready(p: ProviderId) -> bool {
    match p {
        ProviderId::Mock | ProviderId::Ollama => true,
        ProviderId::Anthropic => env_set("ANTHROPIC_API_KEY") || env_set("ANTHROPIC_AUTH_TOKEN"),
        ProviderId::Custom => true,
        other => provider_env_key(other).map(env_set).unwrap_or(false),
    }
}

pub const COLORS: [&str; 8] = [
    "#ef7d57", "#41a6f6", "#38b764", "#b13e53", "#ffcd75", "#73eff7", "#a7f070", "#5d275d",
];

fn emp(id: &str, name: &str, role: Role, model: &str, color: usize, effort: &str) -> Employee {
    let (i, o) = known_price(model);
    Employee {
        id: id.into(),
        name: name.into(),
        role,
        provider: ProviderId::Mock,
        model: model.into(),
        effort: effort.into(),
        input_price: i,
        output_price: o,
        base_url: String::new(),
        extra_prompt: String::new(),
        skill_text: String::new(),
        skill_file: String::new(),
        color: COLORS[color % COLORS.len()].into(),
        enabled: true,
    }
}

/// Default staff. All start on the free mock provider; switch to real models in the Staff panel.
pub fn default_employees() -> Vec<Employee> {
    vec![
        emp("ceo", "Minh", Role::Ceo, "claude-opus-5-5", 0, "high"),
        emp("ba", "Lan", Role::Ba, "claude-sonnet-5-5", 1, "medium"),
        emp("pm", "Giang", Role::Pm, "claude-sonnet-5-5", 4, "medium"),
        emp("designer", "Hà", Role::Designer, "claude-sonnet-5-5", 7, "medium"),
        emp("architect", "Khoa", Role::Architect, "claude-opus-5-5", 5, "high"),
        emp("dev1", "Hùng", Role::Dev, "claude-sonnet-5-5", 2, "high"),
        emp("dev2", "Quân", Role::Dev, "claude-sonnet-5-5", 6, "high"),
        emp("reviewer", "Linh", Role::Reviewer, "claude-sonnet-5-5", 1, "high"),
        emp("tester", "Vy", Role::Tester, "claude-haiku-5-5", 3, "medium"),
        emp("devops", "Nam", Role::Devops, "claude-haiku-5-5", 5, "medium"),
        emp("secretary", "Thảo", Role::Secretary, "claude-haiku-5-5", 4, "low"),
        emp("marketing", "My", Role::Marketing, "claude-sonnet-5-5", 0, "medium"),
        emp("sales", "Tuấn", Role::Sales, "claude-sonnet-5-5", 6, "medium"),
        emp("support1", "Ngọc", Role::Support, "claude-haiku-5-5", 3, "medium"),
        emp("support2", "Bảo", Role::Support, "claude-haiku-5-5", 1, "medium"),
        emp("hr", "Trang", Role::Hr, "claude-haiku-5-5", 7, "medium"),
        emp("accountant", "Đức", Role::Accountant, "claude-haiku-5-5", 4, "medium"),
        emp("legal", "Hiền", Role::Legal, "claude-sonnet-5-5", 5, "medium"),
    ]
}
