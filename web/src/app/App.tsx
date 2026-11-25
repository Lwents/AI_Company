import { useStore } from "@/store";
import { ErrorBoundary } from "@/shared/ui/ErrorBoundary";
import { DialogHost } from "@/shared/ui/Dialog";
import { Office } from "@/features/office/Office";
import { EmployeeDrawer } from "@/features/staff/EmployeeDrawer";
import { Header } from "./Header";
import { ActivityTicker } from "./ActivityTicker";
import { TABS } from "./tabs";
import "./app.css";

/** Page shell: header, office on the left, tabbed panels on the right. */
export function App() {
  const { state, tab, setTab } = useStore();
  const active = TABS.find((t) => t.id === tab) ?? TABS[0];
  const Panel = active.panel;

  return (
    <div className="app">
      <Header />
      <main>
        <div className="left">
          <ErrorBoundary label="Văn phòng">{state ? <Office /> : <div className="office loading">Đang kết nối backend…</div>}</ErrorBoundary>
          <ErrorBoundary label="Thông tin nhân viên">
            <EmployeeDrawer />
            <ActivityTicker />
          </ErrorBoundary>
        </div>
        <aside>
          <nav>
            {TABS.map((t) => {
              const n = state && t.badge ? t.badge(state) : 0;
              return (
                <button key={t.id} className={t.id === active.id ? "on" : ""} onClick={() => setTab(t.id)}>
                  {t.label}
                  {n > 0 && <span className="badge">{n}</span>}
                </button>
              );
            })}
          </nav>
          <div className="panel">
            {state && (
              <ErrorBoundary key={active.id} label={`Tab ${active.label}`}>
                <Panel />
              </ErrorBoundary>
            )}
          </div>
        </aside>
      </main>
      <DialogHost />
    </div>
  );
}
