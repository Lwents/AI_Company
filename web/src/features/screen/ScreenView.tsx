import { useEffect, useState } from "react";
import { api } from "@/api/http";
import { useStore } from "@/store";
import { Avatar } from "@/shared/Avatar";
import { ROLE_LABEL, STATUS_LABEL } from "@/shared/labels";
import type { ScreenEvent } from "@/api/types";
import { ChatPopup } from "@/features/office/Interact";
import { VSCode } from "./VSCode";
import { Browser } from "./Browser";
import "./screen.css";
import "./vscode.css";
import "./browser.css";

type Code = Extract<ScreenEvent, { app: "code" }>;
type Term = Extract<ScreenEvent, { app: "terminal" }>;
type Web = Extract<ScreenEvent, { app: "browser" }>;
type App = "code" | "browser";

/** Recent screen events of one employee: loaded once, then kept live. */
function useScreen(empId: string) {
  const { onScreen } = useStore();
  const [events, setEvents] = useState<ScreenEvent[]>([]);
  useEffect(() => {
    let alive = true;
    setEvents([]);
    const live: ScreenEvent[] = [];
    const off = onScreen((id, e) => {
      if (id !== empId) return;
      if (!alive) return;
      live.push(e);
      setEvents((cur) => [...cur, e].slice(-60));
    });
    api<{ events: ScreenEvent[] }>("GET", `/api/screen/${empId}`)
      .then((r) => {
        if (!alive) return;
        const seen = new Set(live.map((e) => e.ts));
        setEvents([...r.events.filter((e) => !seen.has(e.ts)), ...live].slice(-60));
      })
      .catch(() => {});
    // "Still watching": the server snaps the browser after each action only while someone looks.
    const ping = setInterval(() => void api("GET", `/api/screen/${empId}`).catch(() => {}), 15_000);
    return () => {
      alive = false;
      off();
      clearInterval(ping);
    };
  }, [empId, onScreen]);
  return events;
}

/** Watch an employee work: their editor, terminal and browser, live, with a chat on the side. */
export function ScreenView() {
  const { state, watching, setWatching } = useStore();
  const [chatOpen, setChatOpen] = useState(() => typeof matchMedia !== "function" || !matchMedia("(max-width: 760px)").matches);
  const id = watching!;
  const events = useScreen(id);
  const emp = state?.employees.find((e) => e.id === id);
  const rt = state?.runtime[id];

  useEffect(() => {
    const onKey = (ev: KeyboardEvent) => {
      const t = ev.target as HTMLElement | null;
      if (ev.key === "Escape" && !(t && ["INPUT", "TEXTAREA"].includes(t.tagName))) setWatching(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setWatching]);

  if (!state || !emp) return null;
  const projectId = rt?.projectId ?? [...events].reverse().find((e) => e.projectId)?.projectId ?? null;
  const project = state.projects.find((x) => x.id === projectId);
  // Colleagues who are working right now: switch between their screens.
  const busy = state.employees.filter((e) => e.enabled && (e.id === id || ["working", "thinking", "waiting"].includes(state.runtime[e.id]?.status ?? "idle")));

  return (
    <div className="screen-overlay" role="dialog" aria-label={`Màn hình của ${emp.name}`} onClick={(ev) => ev.target === ev.currentTarget && setWatching(null)}>
      <div className="screen-shell">
        <div className="screen-top">
          <div className="row gap6 screen-who">
            <Avatar e={emp} small />
            <div>
              <b>
                🖥 Màn hình của {emp.name} · {ROLE_LABEL[emp.role]}
              </b>
              <div className="small muted">
                {rt?.note && rt.note.startsWith(STATUS_LABEL[rt.status]) ? rt.note : `${STATUS_LABEL[rt?.status ?? "idle"]}${rt?.note ? ` · ${rt.note}` : ""}`}
              </div>
            </div>
          </div>
          <div className="row gap6">
            <button className="ghost" onClick={() => setChatOpen((v) => !v)}>
              {chatOpen ? "Ẩn chat" : "💬 Chat"}
            </button>
            <button className="ghost" onClick={() => setWatching(null)} aria-label="Đóng">
              ✖
            </button>
          </div>
        </div>
        {busy.length > 1 && (
          <div className="screen-people" aria-label="Nhân viên đang làm việc">
            {busy.map((e) => (
              <button key={e.id} className={e.id === id ? "on" : ""} onClick={() => setWatching(e.id)} title={state.runtime[e.id]?.note ?? ""}>
                <Avatar e={e} small /> {e.name}
              </button>
            ))}
          </div>
        )}
        <div className="screen-body">
          <div className="monitor">
            <Desktop key={id} events={events} busy={rt && rt.status !== "idle" ? rt.note || STATUS_LABEL[rt.status] : null} empId={id} empName={emp.name} projectId={projectId} projectName={project?.slug ?? "du-an"} />
          </div>
          {chatOpen && (
            <div className="screen-chat">
              <ChatPopup empId={id} embedded onClose={() => setChatOpen(false)} />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function Desktop({ events, busy, empId, empName, projectId, projectName }: { events: ScreenEvent[]; busy: string | null; empId: string; empName: string; projectId: string | null; projectName: string }) {
  const codes = events.filter((e): e is Code => e.app === "code");
  const terms = events.filter((e): e is Term => e.app === "terminal");
  const webs = events.filter((e): e is Web => e.app === "browser");
  const latest = [...events].reverse().find((e) => e.app !== "note");
  const auto: App = latest?.app === "browser" ? "browser" : "code";
  const [pinned, setPinned] = useState<App | null>(null);
  const app = pinned ?? auto;
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 15_000);
    return () => clearInterval(t);
  }, []);

  return (
    <div className="desktop">
      <div className="desk-area">
        {events.length === 0 ? (
          <div className="desk-idle">
            <div className="desk-logo">AI</div>
            <p>{busy ? `${empName}: ${busy}` : `Máy của ${empName} đang chờ việc.`}</p>
            <p className="small">Màn hình sẽ hiện code, lệnh và trang web ngay khi {empName} bắt đầu thao tác.</p>
          </div>
        ) : app === "code" ? (
          <VSCode codes={codes} terms={terms} projectId={projectId} projectName={projectName} />
        ) : (
          <Browser webs={webs} empId={empId} empName={empName} projectId={projectId} />
        )}
      </div>
      <div className="taskbar">
        <span className="tb-start">AI</span>
        <button className={app === "code" ? "on" : ""} onClick={() => setPinned("code")} title="Trình soạn code + terminal">
          {"</>"} Code
        </button>
        <button className={app === "browser" ? "on" : ""} onClick={() => setPinned("browser")} title="Trình duyệt">
          🌐 Trình duyệt
        </button>
        <button className={pinned ? "" : "on"} onClick={() => setPinned(null)} title="Tự chuyển theo việc đang làm">
          ⟳ Tự động
        </button>
        <span className="tb-clock">{now.toLocaleTimeString("vi-VN", { hour: "2-digit", minute: "2-digit" })}</span>
      </div>
    </div>
  );
}
