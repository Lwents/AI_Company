import { useState } from "react";
import { useStore } from "@/store";
import { TICKET_COLUMNS } from "@/shared/labels";
import "./kanban.css";

export function KanbanPanel() {
  const { state, selectedProject } = useStore();
  const [open, setOpen] = useState<string | null>(null);
  if (!state) return null;
  if (!selectedProject) return <p className="muted center pad">Chọn một dự án ở tab Dự án.</p>;
  const tickets = state.tickets
    .filter((t) => t.projectId === selectedProject)
    .sort((x, y) => (x.priority ?? 2) - (y.priority ?? 2) || x.createdAt - y.createdAt);
  const name = (id?: string) => state.employees.find((e) => e.id === id);
  if (tickets.length === 0) return <p className="muted center pad">Chưa có ticket. BA sẽ tạo sau khi CEO lập kế hoạch.</p>;

  return (
    <div className="panel-body">
      {TICKET_COLUMNS.map((col) => {
        const list = tickets.filter((t) => t.status === col.id);
        if (list.length === 0 && (col.id === "blocked" || col.id === "review")) return null;
        return (
          <section key={col.id} className="kcol">
            <h4 style={{ borderColor: col.color }}>
              {col.label} <span className="muted">{list.length}</span>
            </h4>
            {list.map((t) => {
              const who = name(t.assignee);
              return (
                <div key={t.id} className="ticket" onClick={() => setOpen(open === t.id ? null : t.id)}>
                  <div className="row between">
                    <span className="row gap4">
                      <span className="key">{t.key}</span>
                      <span className={`prio p${t.priority ?? 2}`}>P{t.priority ?? 2}</span>
                      {t.type === "bug" && <span className="prio bug">BUG</span>}
                    </span>
                    <span className="row gap4">
                      {t.bounces > 0 && <span className="pill red" title="Số lần bị trả lại">↺ {t.bounces}</span>}
                      {who && (
                        <span className="chip" style={{ background: who.color }}>
                          {who.name}
                        </span>
                      )}
                    </span>
                  </div>
                  <div className="ttitle">{t.title}</div>
                  {open === t.id && (
                    <div className="small tdetail">
                      <p>{t.description}</p>
                      {t.acceptance.length > 0 && (
                        <>
                          <b>Tiêu chí nghiệm thu</b>
                          <ul>
                            {t.acceptance.map((a, i) => (
                              <li key={i}>{a}</li>
                            ))}
                          </ul>
                        </>
                      )}
                      {t.notes.length > 0 && (
                        <>
                          <b>Lịch sử</b>
                          <ul>
                            {t.notes.map((n, i) => (
                              <li key={i}>{n}</li>
                            ))}
                          </ul>
                        </>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </section>
        );
      })}
    </div>
  );
}
