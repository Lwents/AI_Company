import { useState } from "react";
import { useStore } from "@/store";
import { ErrorBoundary } from "@/shared/ui/ErrorBoundary";
import { DialogHost } from "@/shared/ui/Dialog";
import { Office } from "@/features/office/Office";
import { EmployeeDrawer } from "@/features/staff/EmployeeDrawer";
import { GuideModal } from "@/features/guide/GuideModal";
import { ScreenView } from "@/features/screen/ScreenView";
import { Header } from "./Header";
import { ActivityTicker } from "./ActivityTicker";
import { TABS } from "./tabs";
import "./app.css";

/** Page shell: header, office on the left, tabbed panels on the right. */
export function App() {
  const { state, tab, setTab, watching } = useStore();
  const active = TABS.find((t) => t.id === tab) ?? TABS[0];
  const Panel = active.panel;
  // Phones show one thing at a time: the office or a panel (the "Văn phòng" tab only exists there).
  const [view, setView] = useState<"office" | "panel">("panel");
  const [guideOpen, setGuideOpen] = useState(false);

  return (
    <div className="app">
      <Header />
      <main className={`view-${view}`}>
        <div className="left">
          <ErrorBoundary label="Văn phòng">
            {state ? <Office onOpenGuide={() => setGuideOpen(true)} /> : <div className="office loading">Đang kết nối backend…</div>}
          </ErrorBoundary>
          <ErrorBoundary label="Thông tin nhân viên">
            <EmployeeDrawer />
            <ActivityTicker />
          </ErrorBoundary>
        </div>
        <aside>
          <nav>
            <button className={`only-mobile ${view === "office" ? "on" : ""}`} onClick={() => setView("office")}>
              Văn phòng
            </button>
            {TABS.map((t) => {
              const n = state && t.badge ? t.badge(state) : 0;
              return (
                <button
                  key={t.id}
                  className={t.id === active.id && view === "panel" ? "on" : ""}
                  onClick={() => {
                    setTab(t.id);
                    setView("panel");
                  }}
                >
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
      {guideOpen && <GuideModal onClose={() => setGuideOpen(false)} />}
      {watching && (
        <ErrorBoundary label="Màn hình nhân viên">
          <ScreenView />
        </ErrorBoundary>
      )}
      <DialogHost />
    </div>
  );
}
