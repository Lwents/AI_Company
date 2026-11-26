import type { CompanyState, Employee, Kpi, UsageStat } from "@/api/types";

export const EMPTY_KPI: Kpi = {
  jobs: 0,
  errors: 0,
  workMs: 0,
  ticketsDone: 0,
  ticketsReturned: 0,
  bugsFound: 0,
  reviews: 0,
  changesRequested: 0,
  ticketsCreated: 0,
  filesWritten: 0,
  casesResolved: 0,
  replies: 0,
  csatSum: 0,
  csatCount: 0,
  dealsWon: 0,
  revenueVnd: 0,
  campaigns: 0,
  lastActive: 0,
};

export interface KpiRow {
  e: Employee;
  k: Kpi;
  u: UsageStat;
  score: number;
  /** 0..1, null when there is nothing to judge yet. */
  quality: number | null;
  reliability: number | null;
  costPerJob: number;
  advice: string[];
}

/**
 * Score 0–100:
 *  - reliability (jobs without errors) up to 40
 *  - role output up to 40 (tickets passed, bugs found, reviews, docs…)
 *  - quality for devs up to 20 (share of tickets that passed first time)
 */
export function kpiRows(st: CompanyState): KpiRow[] {
  const rows = st.employees.map((e) => {
    const k = { ...EMPTY_KPI, ...(st.kpi?.[e.id] ?? {}) };
    const u = st.usage[e.id] ?? { calls: 0, inputTokens: 0, outputTokens: 0, costUsd: 0 };
    const attempts = k.jobs + k.errors;
    const reliability = attempts ? k.jobs / attempts : null;
    const devTickets = k.ticketsDone + k.ticketsReturned;
    const quality = e.role === "dev" ? (devTickets ? k.ticketsDone / devTickets : null) : null;
    let output = 0;
    switch (e.role) {
      case "dev":
        output = k.ticketsDone * 10;
        break;
      case "tester":
        output = k.bugsFound * 8 + k.jobs * 3;
        break;
      case "reviewer":
        output = k.reviews * 5 + k.changesRequested * 5;
        break;
      case "devops":
      case "security":
        output = k.jobs * 8 + k.bugsFound * 6;
        break;
      case "support":
        output = k.casesResolved * 6 + k.replies * 2 + (k.csatCount ? (k.csatSum / k.csatCount) * 4 : 0);
        break;
      case "sales":
        output = k.dealsWon * 15 + k.replies * 2 + k.casesResolved * 3;
        break;
      case "marketing":
      case "hr":
      case "accountant":
      case "legal":
        output = k.campaigns * 10 + k.filesWritten * 2;
        break;
      case "ba":
      case "pm":
      case "architect":
      case "data_engineer":
        output = k.ticketsCreated * 6 + k.jobs * 5;
        break;
      default:
        output = k.jobs * 6 + k.filesWritten * 2;
    }
    const score = attempts
      ? Math.round((reliability ?? 0) * 40 + Math.min(40, output) + (quality === null ? (e.role === "dev" ? 0 : 20) : quality * 20))
      : 0;
    return { e, k, u, score, quality, reliability, costPerJob: k.jobs ? u.costUsd / k.jobs : 0, advice: [] as string[] };
  });

  // Advice needs company-wide averages.
  const paid = rows.filter((r) => r.k.jobs > 0 && r.u.costUsd > 0);
  const avgCost = paid.length ? paid.reduce((s, r) => s + r.costPerJob, 0) / paid.length : 0;
  for (const r of rows) {
    const { e, k } = r;
    if (!e.enabled) r.advice.push("Đang nghỉ (không làm việc). Bật lại trong tab Nhân sự nếu cần.");
    if (e.provider === "mock") r.advice.push("Đang chạy Mock: chỉ diễn kịch bản mẫu, đổi sang model thật để làm việc thật.");
    if (k.errors >= 2 && (r.reliability ?? 1) < 0.7) r.advice.push("Hay gặp lỗi khi gọi model: kiểm tra khóa API, tên model hoặc tăng số bước tối đa.");
    if (e.role === "dev" && k.ticketsDone + k.ticketsReturned >= 2 && (r.quality ?? 1) < 0.5)
      r.advice.push("Ticket bị trả lại nhiều: cân nhắc nâng lên model mạnh hơn (vd. Claude Opus 5.5) hoặc tuyển thêm Reviewer.");
    if (e.role === "tester" && k.jobs >= 4 && k.bugsFound === 0) r.advice.push("Chưa bắt được lỗi nào sau nhiều lần test: có thể kiểm tra chưa kỹ.");
    if (avgCost > 0 && r.costPerJob > avgCost * 2 && k.jobs >= 2)
      r.advice.push("Chi phí mỗi việc cao gấp đôi mặt bằng: thử model rẻ hơn (Sonnet 5.5 / Haiku 5.5) hoặc giảm effort.");
    if (k.jobs + k.errors === 0 && e.enabled) r.advice.push("Chưa có việc nào.");
    if (e.role === "support" && k.csatCount >= 3 && k.csatSum / k.csatCount < 3.5)
      r.advice.push("Khách chấm điểm hài lòng thấp: cân nhắc model mạnh hơn hoặc bổ sung FAQ / chỉ dẫn riêng.");
    if (r.score >= 85 && k.jobs >= 3) r.advice.push("Làm việc xuất sắc 👍");
  }
  return rows.sort((a, b) => b.score - a.score);
}

/** The employee with the best score (needs some work done), for the crown in the office. */
export function topPerformer(st: CompanyState): string | null {
  const best = kpiRows(st).find((r) => r.e.enabled && r.k.jobs >= 2);
  return best ? best.e.id : null;
}
