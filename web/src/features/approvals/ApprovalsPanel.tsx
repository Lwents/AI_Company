import { useState } from "react";
import { api } from "@/api/http";
import { useStore } from "@/store";
import { time } from "@/shared/format";
import { Markdown } from "@/shared/Markdown";
import { previewUrl } from "@/shared/runtime";
import type { Approval } from "@/api/types";

const KIND = { command: "Chạy lệnh", budget: "Ngân sách", question: "Câu hỏi", deal: "Chốt đơn", hire: "Tuyển dụng" } as const;
const STATUS = { pending: "Đang chờ", approved: "Đã duyệt", denied: "Từ chối", answered: "Đã trả lời" } as const;

export function ApprovalsPanel() {
  const { state } = useStore();
  if (!state) return null;
  const pending = state.approvals.filter((a) => a.status === "pending");
  const history = state.approvals.filter((a) => a.status !== "pending").slice(-30).reverse();
  return (
    <div className="panel-body">
      {pending.length === 0 && <p className="muted center pad">Không có việc nào chờ Chủ tịch duyệt.</p>}
      {pending.map((a) => (
        <PendingCard key={a.id} a={a} />
      ))}
      {history.length > 0 && (
        <section className="card">
          <h3>Lịch sử</h3>
          {history.map((a) => (
            <div key={a.id} className="hist small">
              <span className="muted">{time(a.ts)}</span> <b>{KIND[a.kind]}</b> · {STATUS[a.status]}
              <div className="mono clip">{a.detail}</div>
              {a.answer && <div className="muted">↳ {a.answer}</div>}
            </div>
          ))}
        </section>
      )}
    </div>
  );
}

function PendingCard({ a }: { a: Approval }) {
  if (a.options?.length && a.projectId) return <ChoiceCard a={a} projectId={a.projectId} />;
  return <AnswerCard a={a} />;
}

/** Pick one of several options (design directions) from their screenshots. */
function ChoiceCard({ a, projectId }: { a: Approval; projectId: string }) {
  const { state } = useStore();
  const [picked, setPicked] = useState(a.options![0].id);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const emp = state!.employees.find((e) => e.id === a.employeeId);
  const opt = a.options!.find((o) => o.id === picked)!;
  const send = async (decision: "approve" | "deny", answer: string) => {
    setBusy(true);
    try {
      await api("POST", `/api/approvals/${a.id}`, { decision, answer });
    } finally {
      setBusy(false);
    }
  };
  const choose = () =>
    send("approve", `Chọn phương án ${opt.id.toUpperCase()} — ${opt.title}` + (note.trim() ? `\nGóp ý: ${note.trim()}` : ""));
  return (
    <section className="card pending">
      <div className="row between">
        <b>{a.title}</b>
        <span className="pill amber">Chọn phương án</span>
      </div>
      <p className="small muted">
        Từ {emp?.name ?? a.employeeId} · {time(a.ts)}
      </p>
      <div className="choice-grid" role="radiogroup" aria-label="Phương án">
        {a.options!.map((o) => (
          <div
            key={o.id}
            role="radio"
            aria-checked={o.id === picked}
            tabIndex={0}
            className={`choice${o.id === picked ? " on" : ""}`}
            onClick={() => setPicked(o.id)}
            onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), setPicked(o.id))}
          >
            <div className="choice-shots">
              {o.images.length === 0 && <div className="choice-noimg muted small">Chưa có ảnh chụp, bấm "Mở bản mẫu" để xem</div>}
              {o.images.map((img) => (
                <a key={img} href={previewUrl(projectId, img)} target="_blank" rel="noreferrer" onClick={(e) => e.stopPropagation()} className={img.endsWith("mobile.png") ? "shot mobile" : "shot"}>
                  <img src={previewUrl(projectId, img)} alt={`${o.title}: ${img.endsWith("mobile.png") ? "điện thoại" : "máy tính"}`} loading="lazy" />
                </a>
              ))}
            </div>
            <div className="choice-body">
              <b>
                {o.id.toUpperCase()}. {o.title}
                {o.id === a.options![0].id && <span className="pill small-pill">Đề xuất</span>}
              </b>
              {o.note && <Markdown className="read small" text={o.note} />}
              <a className="small" href={previewUrl(projectId, o.link)} target="_blank" rel="noreferrer" onClick={(e) => e.stopPropagation()}>
                Mở bản mẫu ↗
              </a>
            </div>
          </div>
        ))}
      </div>
      <textarea rows={2} placeholder={`Góp ý thêm cho phương án ${opt.id.toUpperCase()} (không bắt buộc)…`} value={note} onChange={(e) => setNote(e.target.value)} />
      <div className="row">
        <button className="primary" disabled={busy} onClick={choose}>
          Chọn phương án {opt.id.toUpperCase()}
        </button>
        <button className="ghost" disabled={busy} onClick={() => send("deny", "")}>
          Để Designer tự chọn
        </button>
      </div>
    </section>
  );
}

function AnswerCard({ a }: { a: Approval }) {
  const { state } = useStore();
  const [answer, setAnswer] = useState("");
  const emp = state!.employees.find((e) => e.id === a.employeeId);
  const decide = (decision: "approve" | "deny") => api("POST", `/api/approvals/${a.id}`, { decision, answer });
  return (
    <section className="card pending">
      <div className="row between">
        <b>{a.title}</b>
        <span className="pill amber">{KIND[a.kind]}</span>
      </div>
      <p className="small muted">
        Từ {emp?.name ?? a.employeeId} · {time(a.ts)}
      </p>
      {a.kind === "command" ? <pre className="code">{a.detail}</pre> : <Markdown className="read" text={a.detail} />}
      {a.kind === "question" ? (
        <>
          <textarea rows={3} placeholder="Trả lời CEO…" value={answer} onChange={(e) => setAnswer(e.target.value)} />
          <div className="row">
            <button className="primary" disabled={!answer.trim()} onClick={() => decide("approve")}>
              Trả lời
            </button>
            <button className="ghost" onClick={() => decide("deny")}>
              Để CEO tự quyết
            </button>
          </div>
        </>
      ) : (
        <div className="row">
          <button className="primary" onClick={() => decide("approve")}>
            {a.kind === "budget" ? "Cấp thêm" : a.kind === "deal" ? "Xác nhận đơn" : a.kind === "hire" ? "Đồng ý tuyển" : "Cho phép"}
          </button>
          <button className="ghost danger" onClick={() => decide("deny")}>
            Từ chối
          </button>
        </div>
      )}
    </section>
  );
}
