import type { EmpStatus, Phase, ProjectStatus, ProviderId, Role, TicketStatus } from "@/api/types";

export const ROLE_LABEL: Record<Role, string> = {
  ceo: "CEO",
  pm: "PM",
  ba: "BA",
  architect: "Tech Lead",
  dev: "Dev",
  reviewer: "Reviewer",
  tester: "Tester",
  devops: "DevOps",
  designer: "Designer",
  secretary: "Thư ký",
  marketing: "Marketing",
  sales: "Sales",
  support: "CSKH",
  hr: "Nhân sự",
  accountant: "Kế toán",
  data_engineer: "Data Engineer",
  security: "Security",
  legal: "Pháp chế",
};

export const ROLES = Object.keys(ROLE_LABEL) as Role[];

/** What each role does in the company process. */
export const ROLE_JOB: Record<Role, string> = {
  ceo: "Lập kế hoạch, tổng kết, báo cáo Chủ tịch",
  pm: "Xếp ưu tiên, lập sprint, chủ trì họp daily",
  ba: "Viết yêu cầu và tạo ticket",
  architect: "Thiết kế kiến trúc, quy ước code",
  dev: "Code ticket (nhiều Dev làm song song)",
  reviewer: "Review code trước khi test",
  tester: "Test ticket theo tiêu chí nghiệm thu",
  devops: "Kiểm tra build, viết README, chặn phát hành lỗi",
  designer: "Thiết kế giao diện, bảng màu, design token",
  secretary: "Ghi biên bản các cuộc họp",
  marketing: "Landing page, bài mạng xã hội, email, SEO, chiến dịch",
  sales: "Bảng giá, tư vấn khách, chốt đơn (Chủ tịch xác nhận)",
  support: "Trả lời khách, FAQ, chuyển lỗi cho kỹ thuật, báo lại khách",
  hr: "Đánh giá nhân viên theo KPI, đề xuất tuyển người",
  accountant: "Báo cáo thu – chi – lãi lỗ từ số liệu thật",
  data_engineer: "Mô hình dữ liệu, lưu trữ, migration, sao lưu, chỉ số",
  security: "Kiểm tra bảo mật, lỗ hổng thư viện, bí mật bị lộ; chặn phát hành khi nguy hiểm",
  legal: "Điều khoản sử dụng, chính sách bảo mật, rủi ro pháp lý",
};

/** Departments, for grouping staff. */
export const DEPARTMENTS: Array<{ label: string; roles: Role[] }> = [
  { label: "Ban điều hành", roles: ["ceo", "secretary"] },
  { label: "Sản phẩm & Kỹ thuật", roles: ["pm", "ba", "designer", "architect", "data_engineer", "dev", "reviewer", "tester", "devops", "security"] },
  { label: "Kinh doanh", roles: ["marketing", "sales", "support"] },
  { label: "Hành chính", roles: ["hr", "accountant", "legal"] },
];

export const PROVIDER_LABEL: Record<ProviderId, string> = {
  mock: "Mock (miễn phí, giả lập)",
  anthropic: "Anthropic (Claude)",
  openai: "OpenAI",
  openrouter: "OpenRouter",
  gemini: "Google Gemini",
  ollama: "Ollama (máy local)",
  custom: "Tùy chỉnh (OpenAI-compatible)",
};

export const PROVIDER_ENV: Partial<Record<ProviderId, string>> = {
  anthropic: "ANTHROPIC_API_KEY",
  openai: "OPENAI_API_KEY",
  openrouter: "OPENROUTER_API_KEY",
  gemini: "GEMINI_API_KEY",
  custom: "CUSTOM_API_KEY",
};

/** Claude models with first-party prices (USD per 1M tokens: input, output). */
export const CLAUDE_MODELS: Array<{ id: string; label: string; price: [number, number] }> = [
  { id: "claude-fable-5-2", label: "Claude Fable 5.2 (mạnh nhất)", price: [8, 40] },
  { id: "claude-opus-5-5", label: "Claude Opus 5.5", price: [4, 20] },
  { id: "claude-sonnet-5-5", label: "Claude Sonnet 5.5", price: [2, 10] },
  { id: "claude-haiku-5-5", label: "Claude Haiku 5.5 (rẻ, nhanh)", price: [0.1, 0.5] },
];

export const STATUS_LABEL: Record<EmpStatus, string> = {
  idle: "Rảnh",
  thinking: "Đang suy nghĩ",
  working: "Đang làm",
  meeting: "Đang họp",
  waiting: "Đang chờ",
  blocked: "Bị chặn",
};

export const TICKET_COLUMNS: Array<{ id: TicketStatus; label: string; color: string }> = [
  { id: "todo", label: "Cần làm", color: "#8a94a8" },
  { id: "in_progress", label: "Đang làm", color: "#5b8def" },
  { id: "review", label: "Review", color: "#a77bf3" },
  { id: "testing", label: "Đang test", color: "#e0b04a" },
  { id: "done", label: "Xong", color: "#4cc38a" },
  { id: "blocked", label: "Bị chặn", color: "#e46a6a" },
];

export const PHASES: Array<{ id: Phase; label: string }> = [
  { id: "plan", label: "Kế hoạch" },
  { id: "requirements", label: "Yêu cầu" },
  { id: "planning", label: "Sprint" },
  { id: "design", label: "Thiết kế" },
  { id: "architecture", label: "Kiến trúc" },
  { id: "build", label: "Làm & test" },
  { id: "release", label: "Phát hành" },
  { id: "report", label: "Báo cáo" },
  { id: "awaiting_approval", label: "Chờ duyệt" },
  { id: "launch", label: "Ra mắt" },
  { id: "operating", label: "Vận hành" },
];

export const PROJECT_STATUS: Record<ProjectStatus, { label: string; tone: string }> = {
  running: { label: "Đang chạy", tone: "blue" },
  paused: { label: "Tạm dừng", tone: "amber" },
  stopped: { label: "Đã dừng", tone: "muted" },
  awaiting_approval: { label: "Chờ Chủ tịch duyệt", tone: "violet" },
  operating: { label: "Đang vận hành", tone: "green" },
  done: { label: "Hoàn thành", tone: "green" },
  failed: { label: "Lỗi", tone: "red" },
};

export const CASE_STATUS: Record<string, { label: string; tone: string }> = {
  open: { label: "Chờ trả lời", tone: "red" },
  in_progress: { label: "Đang trả lời", tone: "blue" },
  waiting_customer: { label: "Chờ khách", tone: "" },
  escalated: { label: "Kỹ thuật đang sửa", tone: "amber" },
  resolved: { label: "Đã xong", tone: "green" },
};
export const CASE_KIND: Record<string, string> = { support: "Hỗ trợ", bug: "Báo lỗi", sales: "Mua hàng" };
