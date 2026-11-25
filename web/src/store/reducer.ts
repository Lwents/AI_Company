import type { CompanyState, ProviderId, ServerEvent } from "@/api/types";

export interface Data {
  state: CompanyState | null;
  providers: Partial<Record<ProviderId, boolean>>;
}

export const initialData: Data = { state: null, providers: {} };

/** How many items of a growing list the browser keeps in memory. */
const CAP: Partial<Record<string, number>> = { activity: 1500, messages: 600 };

/** Applies one server event to the company snapshot. */
export function reducer(s: Data, e: ServerEvent): Data {
  if (e.type === "state") return { state: e.state, providers: e.providers };
  const st = s.state;
  if (!st) return s;
  switch (e.type) {
    case "upsert": {
      const list = (st[e.coll] ?? []) as Array<{ id: string }>;
      const i = list.findIndex((x) => x.id === e.item.id);
      const next = i >= 0 ? list.map((x, j) => (j === i ? e.item : x)) : [...list, e.item];
      const cap = CAP[e.coll];
      return { ...s, state: { ...st, [e.coll]: cap ? next.slice(-cap) : next } };
    }
    case "remove": {
      const list = ((st[e.coll] ?? []) as Array<{ id: string }>).filter((x) => x.id !== e.id);
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
