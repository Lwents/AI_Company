import { createContext, useContext, useEffect, useMemo, useReducer, useRef, useState, type ReactNode } from "react";
import type { CompanyState, ProviderId, ServerEvent } from "./types";

type Live = { type: "handoff"; from: string; to: string; text: string } | { type: "meeting"; participants: string[]; topic: string; active: boolean };
type LiveListener = (e: Live) => void;

interface Ctx {
  state: CompanyState | null;
  providers: Partial<Record<ProviderId, boolean>>;
  connected: boolean;
  /** Office animations subscribe to handoff/meeting events here. */
  onLive: (fn: LiveListener) => () => void;
  selectedProject: string | null;
  setSelectedProject: (id: string | null) => void;
  selectedEmployee: string | null;
  setSelectedEmployee: (id: string | null) => void;
  tab: string;
  setTab: (t: string) => void;
}

const StoreCtx = createContext<Ctx | null>(null);

type Action = ServerEvent;

function reducer(s: { state: CompanyState | null; providers: Partial<Record<ProviderId, boolean>> }, e: Action) {
  if (e.type === "state") return { state: e.state, providers: e.providers };
  const st = s.state;
  if (!st) return s;
  switch (e.type) {
    case "upsert": {
      const list = (st[e.coll] ?? []) as Array<{ id: string }>;
      const i = list.findIndex((x) => x.id === e.item.id);
      const next = i >= 0 ? list.map((x, j) => (j === i ? e.item : x)) : [...list, e.item];
      const capped = e.coll === "activity" ? next.slice(-1500) : e.coll === "messages" ? next.slice(-600) : next;
      return { ...s, state: { ...st, [e.coll]: capped } };
    }
    case "remove": {
      const list = (st[e.coll] as Array<{ id: string }>).filter((x) => x.id !== e.id);
      const extra = e.coll === "projects" ? { tickets: st.tickets.filter((t) => t.projectId !== e.id) } : {};
      return { ...s, state: { ...st, [e.coll]: list, ...extra } };
    }
    case "runtime":
      return { ...s, state: { ...st, runtime: { ...st.runtime, [e.id]: e.runtime } } };
    case "usage":
      return { ...s, state: { ...st, usage: { ...st.usage, [e.id]: e.usage } } };
    case "kpi":
      return { ...s, state: { ...st, kpi: { ...(st.kpi ?? {}), [e.id]: e.kpi } } };
    case "settings":
      return { ...s, state: { ...st, settings: e.settings } };
    default:
      return s;
  }
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const [data, dispatch] = useReducer(reducer, { state: null, providers: {} });
  const [connected, setConnected] = useState(false);
  const listeners = useRef(new Set<LiveListener>());
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [selectedEmployee, setSelectedEmployee] = useState<string | null>(null);
  const [tab, setTab] = useState(() => {
    try {
      return localStorage.getItem("aico.tab") || "projects";
    } catch {
      return "projects";
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem("aico.tab", tab);
    } catch {
      /* storage unavailable */
    }
  }, [tab]);

  useEffect(() => {
    let ws: WebSocket | null = null;
    let retry: number | undefined;
    let closed = false;
    const connect = () => {
      const proto = location.protocol === "https:" ? "wss" : "ws";
      ws = new WebSocket(`${proto}://${location.host}/ws`);
      ws.onopen = () => setConnected(true);
      ws.onmessage = (m) => {
        const e = JSON.parse(m.data) as ServerEvent;
        if (e.type === "handoff" || e.type === "meeting") {
          for (const fn of listeners.current) fn(e);
          return;
        }
        dispatch(e);
      };
      ws.onclose = () => {
        setConnected(false);
        if (!closed) retry = window.setTimeout(connect, 1500);
      };
    };
    connect();
    return () => {
      closed = true;
      window.clearTimeout(retry);
      ws?.close();
    };
  }, []);

  // Default to the most recent project.
  useEffect(() => {
    if (!data.state) return;
    if (selectedProject && data.state.projects.some((p) => p.id === selectedProject)) return;
    const last = [...data.state.projects].sort((a, b) => b.createdAt - a.createdAt)[0];
    setSelectedProject(last?.id ?? null);
  }, [data.state, selectedProject]);

  const value = useMemo<Ctx>(
    () => ({
      state: data.state,
      providers: data.providers,
      connected,
      onLive: (fn) => {
        listeners.current.add(fn);
        return () => {
          listeners.current.delete(fn);
        };
      },
      selectedProject,
      setSelectedProject,
      selectedEmployee,
      setSelectedEmployee,
      tab,
      setTab,
    }),
    [data, connected, selectedProject, selectedEmployee, tab],
  );
  return <StoreCtx.Provider value={value}>{children}</StoreCtx.Provider>;
}

export function useStore() {
  const c = useContext(StoreCtx);
  if (!c) throw new Error("StoreProvider missing");
  return c;
}

export async function api<T = any>(method: string, url: string, body?: unknown): Promise<T> {
  const res = await fetch(url, {
    method,
    headers: body !== undefined ? { "content-type": "application/json" } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  if (!res.ok) throw new Error(text || res.statusText);
  return text ? JSON.parse(text) : (undefined as T);
}
