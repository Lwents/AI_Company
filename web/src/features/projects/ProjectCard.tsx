import { useState } from "react";
import { Markdown } from "@/shared/Markdown";
import { api } from "@/api/http";
import { useStore } from "@/store";
import { confirmDialog } from "@/shared/ui/Dialog";
import { PHASES, PROJECT_STATUS } from "@/shared/labels";

/** Steps the fast lane skips (one engineer builds straight through). */
const FAST_SKIPS: string[] = ["requirements", "planning", "design", "architecture"];
import { usd } from "@/shared/format";
import type { Project } from "@/api/types";
import { previewUrl } from "@/shared/runtime";
import { TeamRow } from "./TeamRow";

/** Last part of a path / URL (the full value is shown on hover). */
function shortSource(src: string): string {
  const parts = src.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts.length > 1 ? `…/${parts[parts.length - 1]}` : src;
}

export function ProjectCard({ p, selected, onSelect }: { p: Project; selected: boolean; onSelect: () => void }) {
  const { state } = useStore();
  const [feedback, setFeedback] = useState("");
  const [err, setErr] = useState("");
  const [check, setCheck] = useState<{ busy: boolean; issues: string[] | null }>({ busy: false, issues: null });
  const runCheck = async () => {
    setCheck({ busy: true, issues: null });
    try {
      const r = await api<{ issues: string[] }>("POST", `/api/projects/${p.id}/check`);
      setCheck({ busy: false, issues: r.issues });
    } catch (e) {
      setCheck({ busy: false, issues: null });
      setErr(String((e as Error).message));
    }
  };
  const st = PROJECT_STATUS[p.status];
  const phaseIdx = PHASES.findIndex((x) => x.id === p.phase);
  const tickets = state!.tickets.filter((t) => t.projectId === p.id);
  const done = tickets.filter((t) => t.status === "done").length;
  const pct = p.budgetUsd > 0 ? Math.min(100, (p.spentUsd / p.budgetUsd) * 100) : 0;

  const act = async (action: string, body?: unknown) => {
    setErr("");
    try {
      await api("POST", `/api/projects/${p.id}/${action}`, body);
      if (action === "changes") setFeedback("");
    } catch (e) {
      setErr(String((e as Error).message));
    }
  };
  const remove = async () => {
    const ok = await confirmDialog({
      title: `Xóa dự án "${p.name}"?`,
      message: "Dự án sẽ biến mất khỏi danh sách. Các tệp trong thư mục workspace vẫn được giữ lại.",
      okLabel: "Xóa dự án",
      danger: true,
    });
    if (!ok) return;
    try {
      await api("DELETE", `/api/projects/${p.id}`);
    } catch (e) {
      setErr(String((e as Error).message));
    }
  };

  return (
    <section className={`card project ${selected ? "selected" : ""}`} onClick={onSelect}>
      <div className="row between">
        <h3 className="clip">{p.name}</h3>
        <span className="row">
          {p.mode === "fast" && (
            <span className="pill mode-fast" title="Chế độ làm nhanh: một kỹ sư làm liền một mạch, không họp / đặc tả / nhiều mẫu thiết kế">
              ⚡ Nhanh
            </span>
          )}
          <span className={`pill ${st.tone}`}>{st.label}</span>
        </span>
      </div>
      <p className="muted small clip2">{p.request}</p>
      <div className="phases">
        {PHASES.map((ph, i) => (
          <span
            key={ph.id}
            className={p.mode === "fast" && FAST_SKIPS.includes(ph.id) ? "skip" : i < phaseIdx ? "done" : i === phaseIdx ? "now" : ""}
            title={p.mode === "fast" && FAST_SKIPS.includes(ph.id) ? `${ph.label} (bỏ qua ở chế độ nhanh)` : ph.label}
          >
            {ph.label}
          </span>
        ))}
      </div>
      <div className="row between small">
        <span>
          Ticket: {done}/{tickets.length}
        </span>
        <span>
          Chi phí {usd(p.spentUsd)} / {usd(p.budgetUsd)}
        </span>
      </div>
      <div className="bar">
        <div style={{ width: `${pct}%` }} className={pct > 90 ? "hot" : ""} />
      </div>
      {p.team?.length > 0 && <TeamRow team={p.team} />}
      {p.error && <p className="error small">{p.error}</p>}

      {selected && (
        <div className="stack" onClick={(e) => e.stopPropagation()}>
          <div className="src-row">
            <span className="src-info small muted" title={p.existing ? p.source : undefined}>
              {p.existing ? (
                <>
                  {p.dir ? "Thư mục" : "Mã nguồn"}: <b>{shortSource(p.source ?? "")}</b>
                </>
              ) : p.tech ? (
                `Công nghệ: ${p.tech}`
              ) : (
                "Công nghệ: PM tư vấn với bạn"
              )}
            </span>
            <button onClick={runCheck} disabled={check.busy} title="Mở bản xem thử bằng trình duyệt thật (máy tính + điện thoại) và tìm lỗi người dùng sẽ gặp">
              {check.busy ? "Đang kiểm tra…" : "🔍 Kiểm tra tự động"}
            </button>
            <a className="btn" href={`/api/projects/${p.id}/download`} download>
              ⬇ Tải mã nguồn (.zip)
            </a>
          </div>
          {check.issues && (
            <div className={`check-result small ${check.issues.length ? "bad" : "good"}`}>
              {check.issues.length === 0 ? (
                "✅ Kiểm tra tự động: không thấy tệp 404, lỗi JavaScript, nút không nhìn thấy hay tràn ngang."
              ) : (
                <>
                  <b>⚠ Kiểm tra tự động thấy {check.issues.length} lỗi:</b>
                  <ul>
                    {check.issues.map((x) => (
                      <li key={x}>{x}</li>
                    ))}
                  </ul>
                </>
              )}
            </div>
          )}
          {p.versions?.length > 0 && (
            <details className="versions">
              <summary>Lịch sử phiên bản ({p.versions.length})</summary>
              <ul>
                {[...p.versions].reverse().map((v) => (
                  <li key={v.n} className="row between small">
                    <span>
                      <b>v{v.n}</b> · {v.label} · {new Date(v.ts).toLocaleString("vi-VN")} · {v.files} tệp · {(v.size / 1024).toFixed(0)} KB
                    </span>
                    <a href={`/api/projects/${p.id}/versions/${v.n}`} download>
                      ⬇ zip
                    </a>
                  </li>
                ))}
              </ul>
            </details>
          )}
          {p.phase === "awaiting_approval" && (
            <div className="report">
              <b>Báo cáo của CEO</b>
              {p.report ? <Markdown className="read" text={p.report} /> : <p>(trống)</p>}
              <div className="row">
                <a className="btn" href={previewUrl(p.id)} target="_blank" rel="noreferrer">
                  Mở sản phẩm
                </a>
                <button className="primary" onClick={() => act("approve")}>
                  Duyệt & ra mắt
                </button>
              </div>
              <textarea rows={2} placeholder="Hoặc yêu cầu sửa: cần thay đổi gì?" value={feedback} onChange={(e) => setFeedback(e.target.value)} />
              <button disabled={!feedback.trim()} onClick={() => act("changes", { feedback })}>
                Gửi yêu cầu sửa
              </button>
            </div>
          )}
          {(p.status === "operating" || (p.phase === "operating" && p.status === "paused")) && (
            <div className="report">
              <b>Đang vận hành</b>
              <p className="small">Khách hàng liên hệ qua cổng hỗ trợ; CSKH, Sales và đội kỹ thuật xử lý tự động. Xem tab Vận hành.</p>
              <div className="row wrap">
                <a className="btn" href={`/portal/${p.id}/`} target="_blank" rel="noreferrer">
                  Cổng khách hàng
                </a>
                <a className="btn" href={previewUrl(p.id)} target="_blank" rel="noreferrer">
                  Sản phẩm
                </a>
              </div>
              <textarea rows={2} placeholder="Muốn nâng cấp sản phẩm? Ghi yêu cầu (đội sẽ làm phiên bản mới rồi ra mắt lại)" value={feedback} onChange={(e) => setFeedback(e.target.value)} />
              <button disabled={!feedback.trim()} onClick={() => act("changes", { feedback })}>
                Giao nâng cấp
              </button>
            </div>
          )}
          {p.status === "done" && (
            <div className="row">
              <a className="btn" href={previewUrl(p.id)} target="_blank" rel="noreferrer">
                Mở sản phẩm
              </a>
              <input placeholder="Muốn thêm tính năng? Ghi vào đây" value={feedback} onChange={(e) => setFeedback(e.target.value)} />
              <button disabled={!feedback.trim()} onClick={() => act("changes", { feedback })}>
                Giao thêm
              </button>
            </div>
          )}
          <div className="row">
            {p.status === "running" && <button onClick={() => act("pause")}>Tạm dừng</button>}
            {p.status === "running" && <button onClick={() => act("stop")}>Dừng</button>}
            {(p.status === "paused" || p.status === "stopped" || p.status === "failed") && p.phase !== "operating" && (
              <button className="primary" onClick={() => act("resume")}>
                Tiếp tục
              </button>
            )}
            {p.status !== "running" && (
              <button className="ghost danger" onClick={remove}>
                Xóa
              </button>
            )}
          </div>
          {p.feedback.length > 0 && (
            <details>
              <summary className="small">Các lần yêu cầu sửa ({p.feedback.length})</summary>
              <ol className="small">
                {p.feedback.map((f, i) => (
                  <li key={i}>{f}</li>
                ))}
              </ol>
            </details>
          )}
          {err && <p className="error small">{err}</p>}
        </div>
      )}
    </section>
  );
}
