import { Office } from "./office/Office";
import { useStore } from "./store";
import { PROJECT_STATUS, ROLE_LABEL, STATUS_LABEL, time, usd } from "./labels";
import { Projects } from "./panels/Projects";
import { Kanban } from "./panels/Kanban";
import { Chat } from "./panels/Chat";
import { Staff } from "./panels/Staff";
import { Costs } from "./panels/Costs";
import { Product } from "./panels/Product";
import { Approvals } from "./panels/Approvals";
import { SettingsPanel } from "./panels/SettingsPanel";
import { KpiPanel } from "./panels/KpiPanel";
import { OpsPanel } from "./panels/OpsPanel";
import { ErrorBoundary } from "./ErrorBoundary";
import { DialogHost } from "./Dialog";

const TABS = [
  { id: "projects", label: "Dự án", el: Projects },
  { id: "ops", label: "Vận hành", el: OpsPanel },
  { id: "kanban", label: "Kanban", el: Kanban },
  { id: "chat", label: "Chat", el: Chat },
  { id: "staff", label: "Nhân sự", el: Staff },
  { id: "kpi", label: "KPI", el: KpiPanel },
  { id: "product", label: "Sản phẩm", el: Product },
  { id: "approvals", label: "Duyệt", el: Approvals },
  { id: "costs", label: "Chi phí", el: Costs },
  { id: "settings", label: "Cài đặt", el: SettingsPanel },
];

export function App() {
  const { state, connected, tab, setTab, selectedProject } = useStore();
  const pending = state?.approvals.filter((a) => a.status === "pending").length ?? 0;
  const openCases = state?.cases?.filter((c) => c.status === "open" || c.status === "in_progress").length ?? 0;
  const project = state?.projects.find((p) => p.id === selectedProject);
  const total = state ? Object.values(state.usage).reduce((s, u) => s + u.costUsd, 0) : 0;
  const Active = TABS.find((t) => t.id === tab)?.el ?? Projects;

  return (
    <div className="app">
      <header>
        <div className="brand">
          <span className="logo">AI</span>
          <div>
            <b>AI Company</b>
            <div className="small muted">Công ty phần mềm do AI vận hành</div>
          </div>
        </div>
        <div className="hstats">
          {project && (
            <span className="hproj">
              <span className="clip">{project.name}</span>
              <span className={`pill ${PROJECT_STATUS[project.status].tone}`}>{PROJECT_STATUS[project.status].label}</span>
            </span>
          )}
          <span className="small muted">Tổng chi: {usd(total)}</span>
          <span className={`conn ${connected ? "on" : ""}`}>{connected ? "● Đã kết nối" : "○ Mất kết nối"}</span>
        </div>
      </header>
      <main>
        <div className="left">
          <ErrorBoundary label="Văn phòng">{state ? <Office /> : <div className="office loading">Đang kết nối backend…</div>}</ErrorBoundary>
          <ErrorBoundary label="Thông tin nhân viên">
            <EmployeeDrawer />
            <Ticker />
          </ErrorBoundary>
        </div>
        <aside>
          <nav>
            {TABS.map((t) => (
              <button key={t.id} className={t.id === tab ? "on" : ""} onClick={() => setTab(t.id)}>
                {t.label}
                {t.id === "approvals" && pending > 0 && <span className="badge">{pending}</span>}
                {t.id === "ops" && openCases > 0 && <span className="badge">{openCases}</span>}
              </button>
            ))}
          </nav>
          <div className="panel">
            {state && (
              <ErrorBoundary key={tab} label={`Tab ${TABS.find((t) => t.id === tab)?.label ?? ""}`}>
                <Active />
              </ErrorBoundary>
            )}
          </div>
        </aside>
      </main>
      <DialogHost />
    </div>
  );
}

function EmployeeDrawer() {
  const { state, selectedEmployee, setSelectedEmployee, setTab } = useStore();
  const e = state?.employees.find((x) => x.id === selectedEmployee);
  if (!state || !e) return null;
  const rt = state.runtime[e.id];
  const u = state.usage[e.id];
  const acts = state.activity.filter((a) => a.employeeId === e.id).slice(-40).reverse();
  return (
    <div className="drawer">
      <div className="row between">
        <div className="row gap6">
          <span className="avatar" style={{ background: e.color }}>
            {e.name.slice(0, 1)}
          </span>
          <div>
            <b>{e.name}</b> <span className="muted">· {ROLE_LABEL[e.role]}</span>
            <div className="small muted">
              {e.provider === "mock" ? "Mock (giả lập)" : `${e.provider} · ${e.model}`} · {u ? `${u.calls} lượt · ${usd(u.costUsd)}` : "chưa làm việc"}
            </div>
          </div>
        </div>
        <div className="row gap4">
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

function Ticker() {
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
