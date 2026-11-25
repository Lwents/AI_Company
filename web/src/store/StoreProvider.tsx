import { createContext, useContext, useEffect, useMemo, useReducer, useRef, useState, type ReactNode } from "react";
import type { CompanyState, ProviderId, ServerEvent } from "@/api/types";
import { connectSocket } from "@/api/socket";
import { initialData, reducer } from "./reducer";

/** Animation-only events: not stored, just forwarded to the office. */
export type LiveEvent = Extract<ServerEvent, { type: "handoff" } | { type: "meeting" }>;
type LiveListener = (e: LiveEvent) => void;

interface StoreValue {
  state: CompanyState | null;
  providers: Partial<Record<ProviderId, boolean>>;
  connected: boolean;
  /** Subscribe to handoff/meeting events. Returns an unsubscribe function. */
  onLive: (fn: LiveListener) => () => void;
  selectedProject: string | null;
  setSelectedProject: (id: string | null) => void;
  selectedEmployee: string | null;
  setSelectedEmployee: (id: string | null) => void;
  tab: string;
  setTab: (t: string) => void;
}

const StoreCtx = createContext<StoreValue | null>(null);

const TAB_KEY = "aico.tab";

function loadTab() {
  try {
    return localStorage.getItem(TAB_KEY) || "projects";
  } catch {
    return "projects";
  }
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const [data, dispatch] = useReducer(reducer, initialData);
  const [connected, setConnected] = useState(false);
  const listeners = useRef(new Set<LiveListener>());
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [selectedEmployee, setSelectedEmployee] = useState<string | null>(null);
  const [tab, setTab] = useState(loadTab);
  // Stable identity, so subscribers don't resubscribe on every state change.
  const onLive = useMemo(
    () => (fn: LiveListener) => {
      listeners.current.add(fn);
      return () => {
        listeners.current.delete(fn);
      };
    },
    [],
  );

  useEffect(() => {
    try {
      localStorage.setItem(TAB_KEY, tab);
    } catch {
      /* storage unavailable */
    }
  }, [tab]);

  useEffect(() => {
    return connectSocket((e) => {
      if (e.type === "handoff" || e.type === "meeting") {
        for (const fn of listeners.current) fn(e);
      } else {
        dispatch(e);
      }
    }, setConnected);
  }, []);

  // Default to the most recent project.
  useEffect(() => {
    if (!data.state) return;
    if (selectedProject && data.state.projects.some((p) => p.id === selectedProject)) return;
    const last = [...data.state.projects].sort((a, b) => b.createdAt - a.createdAt)[0];
    setSelectedProject(last?.id ?? null);
  }, [data.state, selectedProject]);

  const value = useMemo<StoreValue>(
    () => ({
      state: data.state,
      providers: data.providers,
      connected,
      onLive,
      selectedProject,
      setSelectedProject,
      selectedEmployee,
      setSelectedEmployee,
      tab,
      setTab,
    }),
    [data, connected, onLive, selectedProject, selectedEmployee, tab],
  );
  return <StoreCtx.Provider value={value}>{children}</StoreCtx.Provider>;
}

export function useStore() {
  const c = useContext(StoreCtx);
  if (!c) throw new Error("StoreProvider missing");
  return c;
}
