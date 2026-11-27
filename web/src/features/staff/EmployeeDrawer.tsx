import { useStore } from "@/store";
import { Avatar } from "@/shared/Avatar";
import { ROLE_LABEL, STATUS_LABEL } from "@/shared/labels";
import { time, usd } from "@/shared/format";
import "./staff.css";

export function EmployeeDrawer() {
  const { state, selectedEmployee, setSelectedEmployee, setTab , setWatching } = useStore();
  const e = state?.employees.find((x) => x.id === selectedEmployee);
  if (!state || !e) return null;
  const rt = state.runtime[e.id];
  const u = state.usage[e.id];
  const acts = state.activity.filter((a) => a.employeeId === e.id).slice(-40).reverse();
  return (
    <div className="drawer">
      <div className="row between">
        <div className="row gap6">
          <Avatar e={e} />
          <div>
            <b>{e.name}</b> <span className="muted">· {ROLE_LABEL[e.role]}</span>
            <div className="small muted">
              {e.provider === "mock" ? "Mock (giả lập)" : `${e.provider} · ${e.model}`} · {u ? `${u.calls} lượt · ${usd(u.costUsd)}` : "chưa làm việc"}
            </div>
          </div>
        </div>
        <div className="row gap4">
          <button className="ghost" onClick={() => setWatching(e.id)} title="Xem màn hình máy tính của nhân viên">
            🖥 Màn hình
          </button>
          <button className="ghost" onClick={() => setTab("staff")}>
            Sửa
          </button>
          <button className="ghost" onClick={() => setSelectedEmployee(null)}>
            ✕
          </button>
        </div>
      </div>
      <p className="small">
        <span className={`status ${rt?.status ?? "idle"}`}>{STATUS_LABEL[rt?.status ?? "idle"]}</span> {rt?.note}
        {rt?.projectId && rt.status !== "idle" && (
          <>
            {" "}
            · Dự án: <b>{state.projects.find((x) => x.id === rt.projectId)?.name ?? "?"}</b>
          </>
        )}
      </p>
      <div className="acts">
        {acts.length === 0 && <p className="muted small">Chưa có hoạt động.</p>}
        {acts.map((a) => (
          <div key={a.id} className={`act ${a.kind}`}>
            <span className="muted">{time(a.ts)}</span> <span>{a.text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
