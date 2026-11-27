import { useState } from "react";
import { api } from "@/api/http";
import { useStore } from "@/store";
import { time } from "@/shared/format";
import { Markdown } from "@/shared/Markdown";
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
