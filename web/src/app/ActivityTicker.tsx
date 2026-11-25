import { useStore } from "@/store";

/** Last few tool calls / errors, scrolling under the office. */
export function ActivityTicker() {
  const { state } = useStore();
  if (!state) return null;
  const last = state.activity.filter((a) => a.kind === "tool" || a.kind === "error").slice(-3).reverse();
  return (
    <div className="ticker">
      {last.length === 0 && <span className="muted small">Văn phòng đang yên tĩnh. Giao một dự án ở tab Dự án để bắt đầu.</span>}
      {last.map((a) => {
        const e = state.employees.find((x) => x.id === a.employeeId);
        return (
          <span key={a.id} className={`small ${a.kind === "error" ? "err" : ""}`}>
            <span className="dot" style={{ background: e?.color }} /> <b>{e?.name}</b> {a.text.slice(0, 70)}
          </span>
        );
      })}
    </div>
  );
}
