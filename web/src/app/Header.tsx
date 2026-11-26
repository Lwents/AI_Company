import { useStore } from "@/store";
import { PROJECT_STATUS } from "@/shared/labels";
import { usd } from "@/shared/format";
import { logout, runtime } from "@/shared/runtime";

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
          <div className="small muted hide-mobile">Công ty phần mềm do AI vận hành</div>
        </div>
      </div>
      <div className="hstats">
        {project && (
          <span className="hproj">
            <span className="clip">{project.name}</span>
            <span className={`pill ${PROJECT_STATUS[project.status].tone}`}>{PROJECT_STATUS[project.status].label}</span>
          </span>
        )}
        <span className="small muted nowrap">
          <span className="hide-mobile">Tổng chi: </span>
          {usd(total)}
        </span>
        <span className={`conn ${connected ? "on" : ""}`} title={connected ? "Đã kết nối" : "Mất kết nối"}>
          {connected ? "●" : "○"}
          <span className="conn-label"> {connected ? "Đã kết nối" : "Mất kết nối"}</span>
        </span>
        {runtime.enabled && (
          <button className="ghost small" onClick={logout} title={runtime.user ? `Đang đăng nhập: ${runtime.user}` : undefined}>
            Đăng xuất
          </button>
        )}
      </div>
    </header>
  );
}
