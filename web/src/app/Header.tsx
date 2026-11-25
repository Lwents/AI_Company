import { useStore } from "@/store";
import { PROJECT_STATUS } from "@/shared/labels";
import { usd } from "@/shared/format";

export function Header() {
  const { state, connected, selectedProject } = useStore();
  const project = state?.projects.find((p) => p.id === selectedProject);
  const total = state ? Object.values(state.usage).reduce((s, u) => s + u.costUsd, 0) : 0;

  return (
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
  );
}
