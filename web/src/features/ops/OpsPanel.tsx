import { useState } from "react";
import { api } from "@/api/http";
import { useStore } from "@/store";
import { confirmDialog } from "@/shared/ui/Dialog";
import { USD_TO_VND, vnd } from "@/shared/format";
import { CaseRow } from "./CaseRow";
import "./ops.css";
import { previewUrl } from "@/shared/runtime";

export function OpsPanel() {
  const { state, selectedProject } = useStore();
  const [brief, setBrief] = useState("");
  const [msg, setMsg] = useState("");
  const [filter, setFilter] = useState<"active" | "all">("active");
  const [open, setOpen] = useState<string | null>(null);
  if (!state) return null;
  const p = state.projects.find((x) => x.id === selectedProject);
  if (!p) return <p className="muted center pad">Chọn một dự án ở tab Dự án.</p>;

  const live = p.phase === "operating";
  const portal = `${location.origin}/portal/${p.id}/`;
  const cases = (state.cases ?? []).filter((c) => c.projectId === p.id).sort((a, b) => b.updatedAt - a.updatedAt);
  const shown = filter === "active" ? cases.filter((c) => c.status !== "resolved") : cases;
  const deals = (state.deals ?? []).filter((d) => d.projectId === p.id);
  const revenue = deals.reduce((s, d) => s + d.amountVnd, 0);
  const costVnd = p.spentUsd * USD_TO_VND;
  const rated = cases.filter((c) => c.rating);
  const csat = rated.length ? rated.reduce((s, c) => s + (c.rating ?? 0), 0) / rated.length : null;

  const act = async (action: string, body?: unknown) => {
    setMsg("");
    try {
      const r = await api<{ message?: string; created?: number }>("POST", `/api/projects/${p.id}/${action}`, body);
      setMsg(r?.message ?? (r?.created ? `Đã tạo ${r.created} khách hàng thử` : "Đã gửi"));
      if (action === "campaign") setBrief("");
    } catch (e) {
      setMsg(String((e as Error).message));
    }
  };

  return (
    <div className="panel-body">
      <section className="card">
        <div className="row between">
          <h3 className="clip">{p.name}</h3>
          <span className={`pill ${live && p.status === "operating" ? "green" : "amber"}`}>
            {live ? (p.status === "operating" ? "Đang vận hành" : "Tạm dừng vận hành") : p.phase === "launch" ? "Đang chuẩn bị ra mắt" : "Chưa ra mắt"}
          </span>
        </div>
        {live && (
          <>
            <p className="small">Cổng khách hàng (gửi link này cho khách):</p>
            <div className="row">
              <input readOnly value={portal} onFocus={(e) => e.target.select()} />
              <a className="btn" href={portal} target="_blank" rel="noreferrer">
                Mở
              </a>
            </div>
            <div className="row wrap">
              <a className="btn" href={previewUrl(p.id)} target="_blank" rel="noreferrer">
                Sản phẩm
              </a>
              <a className="btn" href={previewUrl(p.id, "marketing/landing.html")} target="_blank" rel="noreferrer">
                Landing page
              </a>
              <button onClick={() => act("simulate")}>Tạo 4 khách hàng thử</button>
              {p.status === "operating" ? (
                <button className="ghost" onClick={() => act("pause")}>
                  Tạm dừng
                </button>
              ) : (
                <button className="primary" onClick={() => act("resume")}>
                  Mở lại
                </button>
              )}
              <button
                className="danger"
                onClick={async () => {
                  const ok = await confirmDialog({
                    title: `Ngừng vận hành "${p.name}"?`,
                    message: "Cổng khách hàng sẽ đóng, CSKH và Sales ngừng nhận khách mới. Bạn có thể mở lại sau.",
                    okLabel: "Ngừng vận hành",
                    danger: true,
                  });
                  if (ok) act("close");
                }}
              >
                Ngừng vận hành
              </button>
            </div>
          </>
        )}
      </section>

      <section className="card">
        <h3>Kinh doanh</h3>
        <div className="kpi-grid">
          <div>
            <b>{vnd(revenue)}</b>
            <span>doanh thu ({deals.length} đơn)</span>
          </div>
          <div>
            <b>{vnd(costVnd)}</b>
            <span>chi phí AI</span>
          </div>
          <div>
            <b style={{ color: revenue - costVnd >= 0 ? "var(--lime)" : "#ff8e9e" }}>{vnd(revenue - costVnd)}</b>
            <span>lợi nhuận</span>
          </div>
          <div>
            <b>{csat === null ? "—" : `${csat.toFixed(1)}★`}</b>
            <span>hài lòng ({rated.length})</span>
          </div>
        </div>
        {deals.length > 0 && (
          <table>
            <tbody>
              {deals
                .slice()
                .reverse()
                .map((d) => (
                  <tr key={d.id}>
                    <td>{d.customer}</td>
                    <td className="small">{d.plan}</td>
                    <td>{vnd(d.amountVnd)}</td>
                  </tr>
                ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="card">
        <h3>Giao việc cho các phòng ban</h3>
        <textarea rows={2} placeholder="Ví dụ: Chiến dịch khuyến mãi Tết, giảm 30% gói Pro cho sinh viên…" value={brief} onChange={(e) => setBrief(e.target.value)} />
        <div className="row wrap">
          <button className="primary" disabled={!live && p.phase !== "done"} onClick={() => act("campaign", { feedback: brief })}>
            📣 Chiến dịch marketing
          </button>
          <button disabled={!live && p.phase !== "done"} onClick={() => act("finance", { feedback: brief })}>
            💰 Báo cáo tài chính
          </button>
          <button disabled={!live && p.phase !== "done"} onClick={() => act("hr", { feedback: brief })}>
            👥 Đánh giá nhân sự
          </button>
        </div>
        {msg && <p className="small muted">{msg}</p>}
      </section>

      <section className="card">
        <div className="row between">
          <h3>Hộp thư khách hàng</h3>
          <span className="row gap4">
            <button className={filter === "active" ? "primary" : "ghost"} onClick={() => setFilter("active")}>
              Đang mở {cases.filter((c) => c.status !== "resolved").length}
            </button>
            <button className={filter === "all" ? "primary" : "ghost"} onClick={() => setFilter("all")}>
              Tất cả {cases.length}
            </button>
          </span>
        </div>
        {shown.length === 0 && <p className="muted small">Chưa có hồ sơ nào.</p>}
        {shown.map((c) => (
          <CaseRow key={c.id} c={c} open={open === c.id} onToggle={() => setOpen(open === c.id ? null : c.id)} />
        ))}
      </section>
    </div>
  );
}
