import { useEffect, useState } from "react";
import { useStore } from "@/store";
import { PROJECT_STATUS } from "@/shared/labels";
import { usd } from "@/shared/format";
import { logout, runtime } from "@/shared/runtime";

export function Header() {
  const { state, connected, selectedProject } = useStore();
  const project = state?.projects.find((p) => p.id === selectedProject);
  const total = state
    ? Object.values(state.usage).reduce((s, u) => s + u.costUsd, 0)
    : 0;

  return (
    <>
      <DownBanner connected={connected} />
      <header>
        <div className="brand">
          <span className="logo">AI</span>
          <div>
            <b>AI Company</b>
            <div className="small muted hide-mobile">
              Công ty phần mềm do AI vận hành
            </div>
          </div>
        </div>
        <div className="hstats">
          {project && (
            <span className="hproj">
              <span className="clip">{project.name}</span>
              <span className={`pill ${PROJECT_STATUS[project.status].tone}`}>
                {PROJECT_STATUS[project.status].label}
              </span>
            </span>
          )}
          <span className="small muted nowrap">
            <span className="hide-mobile">Tổng chi: </span>
            {usd(total)}
          </span>
          <span
            className={`conn ${connected ? "on" : ""}`}
            title={connected ? "Đã kết nối" : "Mất kết nối"}
          >
            {connected ? "●" : "○"}
            <span className="conn-label">
              {" "}
              {connected ? "Đã kết nối" : "Mất kết nối"}
            </span>
          </span>
          {runtime.enabled && (
            <button
              className="ghost small"
              onClick={logout}
              title={
                runtime.user ? `Đang đăng nhập: ${runtime.user}` : undefined
              }
            >
              Đăng xuất
            </button>
          )}
        </div>
      </header>
    </>
  );
}

/** After a few seconds without the server (restart, deploy, crash): point to the rescue page, a separate service. */
function DownBanner({ connected }: { connected: boolean }) {
  const [long, setLong] = useState(false);
  useEffect(() => {
    if (connected) {
      setLong(false);
      return;
    }
    const t = setTimeout(() => setLong(true), 8000);
    return () => clearTimeout(t);
  }, [connected]);
  if (!long) return null;
  return (
    <div className="down-banner" role="alert">
      ⏳ Mất kết nối với máy chủ (đang khởi động lại, đang cập nhật hoặc gặp sự
      cố).{" "}
      <a href="/rescue/" target="_blank" rel="noreferrer">
        🛟 Mở trang cứu hộ
      </a>
    </div>
  );
}
