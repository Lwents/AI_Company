import { useStore } from "@/store";
import { kpiRows } from "@/features/kpi/scoring";
import { ROLE_LABEL } from "@/shared/labels";
import { num, usd } from "@/shared/format";
import "./kpi.css";

const pct = (v: number | null) => (v === null ? "—" : `${Math.round(v * 100)}%`);

function grade(score: number) {
  if (score >= 85) return { label: "S", tone: "green" };
  if (score >= 70) return { label: "A", tone: "blue" };
  if (score >= 50) return { label: "B", tone: "amber" };
  if (score > 0) return { label: "C", tone: "red" };
  return { label: "–", tone: "" };
}

export function KpiPanel() {
  const { state, setSelectedEmployee, setTab } = useStore();
  if (!state) return null;
  const rows = kpiRows(state);
  const active = rows.filter((r) => r.k.jobs + r.k.errors > 0);
  const totalJobs = rows.reduce((s, r) => s + r.k.jobs, 0);
  const totalDone = rows.reduce((s, r) => s + r.k.ticketsDone, 0);
  const totalReturned = rows.reduce((s, r) => s + r.k.ticketsReturned, 0);
  const totalBugs = rows.reduce((s, r) => s + r.k.bugsFound, 0);
  const best = active.find((r) => r.e.enabled && r.k.jobs >= 2);

  return (
    <div className="panel-body">
      <section className="card">
        <h3>Toàn công ty</h3>
        <div className="kpi-grid">
          <div>
            <b>{num(totalJobs)}</b>
            <span>lượt việc xong</span>
          </div>
          <div>
            <b>{num(totalDone)}</b>
            <span>ticket đạt</span>
          </div>
          <div>
            <b>{num(totalReturned)}</b>
            <span>lần bị trả lại</span>
          </div>
          <div>
            <b>{num(totalBugs)}</b>
            <span>lỗi bắt được</span>
          </div>
        </div>
        {best && (
          <p className="small">
            👑 Nhân viên xuất sắc: <b style={{ color: "var(--amber)" }}>{best.e.name}</b> ({ROLE_LABEL[best.e.role]}) · {best.score} điểm
          </p>
        )}
      </section>

      <section className="card">
        <h3>Bảng xếp hạng</h3>
        {rows.map((r, i) => {
          const g = grade(r.score);
          return (
            <div
              key={r.e.id}
              className={`rank ${r.e.enabled ? "" : "disabled"}`}
              onClick={() => {
                setSelectedEmployee(r.e.id);
              }}
            >
              <span className="pos">{r.score > 0 ? `#${i + 1}` : "·"}</span>
              <span className="avatar sm" style={{ background: r.e.color }}>
                {r.e.name.slice(0, 1)}
              </span>
              <div className="grow">
                <div className="row between">
                  <span>
                    <b>{r.e.name}</b> <span className="muted">· {ROLE_LABEL[r.e.role]}</span>
                  </span>
                  <span className="row gap4">
                    <span className="score">{r.score}</span>
                    <span className={`pill ${g.tone}`}>{g.label}</span>
                  </span>
                </div>
                <div className="bar thin">
                  <div style={{ width: `${r.score}%` }} />
                </div>
                <div className="small muted stats">
                  <span>Việc {r.k.jobs}</span>
                  <span>Lỗi {r.k.errors}</span>
                  <span>Ổn định {pct(r.reliability)}</span>
                  {r.e.role === "dev" && (
                    <span>
                      Đạt ngay {pct(r.quality)} ({r.k.ticketsDone}/{r.k.ticketsDone + r.k.ticketsReturned})
                    </span>
                  )}
                  {(r.e.role === "tester" || r.e.role === "devops") && <span>Bắt lỗi {r.k.bugsFound}</span>}
                  {r.e.role === "reviewer" && (
                    <span>
                      Review {r.k.reviews} · yêu cầu sửa {r.k.changesRequested}
                    </span>
                  )}
                  {(r.e.role === "ba" || r.e.role === "pm" || r.e.role === "architect") && <span>Tạo ticket {r.k.ticketsCreated}</span>}
                  {(r.e.role === "support" || r.e.role === "sales") && (
                    <span>
                      Hồ sơ xong {r.k.casesResolved ?? 0} · trả lời {r.k.replies ?? 0}
                      {r.k.csatCount ? ` · ${(r.k.csatSum / r.k.csatCount).toFixed(1)}★` : ""}
                    </span>
                  )}
                  {r.e.role === "sales" && (
                    <span>
                      Chốt {r.k.dealsWon ?? 0} đơn · {(r.k.revenueVnd ?? 0).toLocaleString("vi-VN")} đ
                    </span>
                  )}
                  {["marketing", "hr", "accountant", "legal"].includes(r.e.role) && <span>Hoàn thành {r.k.campaigns ?? 0} đầu việc</span>}
                  <span>{usd(r.u.costUsd)}{r.k.jobs ? ` · ${usd(r.costPerJob)}/việc` : ""}</span>
                  {r.k.workMs > 0 && <span>{Math.round(r.k.workMs / 1000)} giây làm việc</span>}
                </div>
                {r.advice.length > 0 && (
                  <ul className="advice">
                    {r.advice.map((a, j) => (
                      <li key={j}>{a}</li>
                    ))}
                  </ul>
                )}
              </div>
            </div>
          );
        })}
        <button className="ghost" onClick={() => setTab("staff")}>
          Sang tab Nhân sự để đổi model / tuyển thêm
        </button>
      </section>
    </div>
  );
}
