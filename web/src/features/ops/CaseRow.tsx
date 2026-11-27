import { useState } from "react";
import { Markdown } from "@/shared/Markdown";
import { api } from "@/api/http";
import { useStore } from "@/store";
import { CASE_KIND, CASE_STATUS, ROLE_LABEL } from "@/shared/labels";
import { time } from "@/shared/format";
import type { SupportCase } from "@/api/types";

export function CaseRow({ c, open, onToggle }: { c: SupportCase; open: boolean; onToggle: () => void }) {
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
                  <Markdown className="read" text={m.text} />
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
