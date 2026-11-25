import { useState } from "react";
import { api, useStore } from "../store";
import { confirmDialog } from "../Dialog";
import { CASE_KIND, CASE_STATUS, ROLE_LABEL, USD_TO_VND, time, vnd } from "../labels";
import type { SupportCase } from "../types";

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
        {!live && p.phase !== "launch" && (
          <p className="small muted">Sản phẩm ra mắt sau khi Chủ tịch duyệt dự án (tab Dự án → Duyệt & ra mắt). Khi đó Marketing, Sales, CSKH, Pháp chế, Kế toán, Nhân sự cùng chuẩn bị.</p>
        )}
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
              <a className="btn" href={`/preview/${p.id}/`} target="_blank" rel="noreferrer">
                Sản phẩm
              </a>
              <a className="btn" href={`/preview/${p.id}/marketing/landing.html`} target="_blank" rel="noreferrer">
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
            <p className="small muted">
              Để khách ở máy khác truy cập: đặt <code>PORTAL_BIND=0.0.0.0:8790</code> trong <code>.env</code> (chỉ mở cổng khách hàng và sản phẩm, không mở phần quản trị).
            </p>
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

function CaseRow({ c, open, onToggle }: { c: SupportCase; open: boolean; onToggle: () => void }) {
  const { state } = useStore();
  const [text, setText] = useState("");
  const st = CASE_STATUS[c.status] ?? { label: c.status, tone: "" };
  const who = (id: string) => {
    if (id === "customer") return { name: c.customer, mine: false, color: "#ffcd75" };
    if (id === "system") return { name: "Hệ thống", mine: true, color: "#94b0c2" };
    if (id === "chairman") return { name: "Chủ tịch (bạn)", mine: true, color: "#ffcd75" };
    const e = state!.employees.find((x) => x.id === id);
    return { name: e ? `${e.name} · ${ROLE_LABEL[e.role]}` : id, mine: true, color: e?.color ?? "#94b0c2" };
  };
  const assignee = c.assignee ? state!.employees.find((e) => e.id === c.assignee) : undefined;
  const ticket = c.ticketId ? state!.tickets.find((t) => t.id === c.ticketId) : undefined;
  const send = async () => {
    if (!text.trim()) return;
    await api("POST", `/api/cases/${c.id}/reply`, { text });
    setText("");
  };

  return (
    <div className={`case ${open ? "on" : ""}`}>
      <div className="row between click" onClick={onToggle}>
        <span className="clip">
          <span className="prio p2">{CASE_KIND[c.kind]}</span> <b>{c.customer}</b> <span className="muted">· {c.subject}</span>
        </span>
        <span className={`pill ${st.tone}`}>{st.label}</span>
      </div>
      <div className="small muted">
        {time(c.updatedAt)}
        {assignee && ` · phụ trách: ${assignee.name}`}
        {ticket && ` · ${ticket.key} (${ticket.status})`}
        {c.rating ? ` · ${"★".repeat(c.rating)}` : ""}
      </div>
      {open && (
        <div className="stack">
          {c.contact && <p className="small">Liên hệ: {c.contact}</p>}
          <div className="thread">
            {c.messages.map((m, i) => {
              const w = who(m.from);
              return (
                <div key={i} className={`bubble ${m.from === "customer" ? "them" : m.from === "system" ? "sys" : "us"}`}>
                  <div className="small" style={{ color: w.color }}>
                    {w.name} · {time(m.ts)}
                  </div>
                  <div className="read">{m.text}</div>
                </div>
              );
            })}
          </div>
          <div className="row">
            <input placeholder="Chủ tịch trả lời trực tiếp…" value={text} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => e.key === "Enter" && send()} />
            <button className="primary" onClick={send} disabled={!text.trim()}>
              Gửi
            </button>
            {c.status !== "resolved" && (
              <button className="ghost" onClick={() => api("POST", `/api/cases/${c.id}/resolve`)}>
                Đóng
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
