import { useCallback, useEffect, useState } from "react";
import { api } from "@/api/http";
import "./resources.css";

interface Account {
  id: string;
  provider: string;
  name?: string;
  email?: string;
  authType?: string;
  isActive?: boolean;
  testStatus?: string;
  lastError?: string;
  rateLimitedUntil?: string;
}
interface Source {
  idx: number;
  url: string;
  label: string;
  accounts: Account[];
  error?: string;
  /** Accounts hidden because an earlier source already lists them. */
  dup?: number;
}
interface QuotaRow {
  model: string;
  name: string;
  used?: number;
  total?: number;
  remainingPct?: number;
  unlimited?: boolean;
  resetAt?: string;
}
interface Quota {
  plan?: string;
  tier?: string;
  message?: string;
  quotas: QuotaRow[];
}
type QState = { loading?: boolean; err?: string; data?: Quota };

const NAMES: Record<string, string> = {
  gemini_session: "Gemini (5h)",
  claude_gpt_session: "Claude (5h)",
  gemini_weekly: "Gemini (tuần)",
  claude_gpt_weekly: "Claude (tuần)",
};
const NO_QUOTA = "Usage not available for this connection";

const fmt = (n?: number) => (typeof n === "number" ? n.toLocaleString("vi-VN") : "?");
const key = (src: number, id: string) => `${src}:${id}`;

function resetIn(iso?: string): string {
  if (!iso) return "";
  const ms = new Date(iso).getTime() - Date.now();
  if (!(ms > 0)) return "đã làm mới";
  const m = Math.round(ms / 60000);
  const d = Math.floor(m / 1440), h = Math.floor((m % 1440) / 60), mm = m % 60;
  return "còn " + (d ? `${d}n ${h}g ${mm}p` : h ? `${h}g ${mm}p` : `${mm}p`);
}

const tone = (pct: number) => (pct <= 10 ? "low" : pct <= 40 ? "mid" : "ok");
const pctOf = (r: QuotaRow) => (r.unlimited ? 100 : Math.max(0, Math.min(100, r.remainingPct ?? 0)));

function status(a: Account): { label: string; cls: string } {
  if (a.rateLimitedUntil && new Date(a.rateLimitedUntil).getTime() > Date.now()) return { label: "Bị giới hạn", cls: "amber" };
  if (a.testStatus && a.testStatus !== "active") return { label: "Lỗi", cls: "red" };
  if (a.isActive === false) return { label: "Tắt", cls: "" };
  return { label: "Đang dùng", cls: "green" };
}

function groupByProvider(list: Account[]): [string, Account[]][] {
  const m = new Map<string, Account[]>();
  for (const a of list) m.set(a.provider, [...(m.get(a.provider) ?? []), a]);
  return [...m.entries()];
}

export function ResourcesPanel() {
  const [sources, setSources] = useState<Source[] | null>(null);
  const [err, setErr] = useState("");
  const [quotas, setQuotas] = useState<Record<string, QState>>({});
  const [updated, setUpdated] = useState<number | null>(null);

  const loadQuota = useCallback(async (src: number, id: string) => {
    const k = key(src, id);
    setQuotas((q) => ({ ...q, [k]: { ...q[k], loading: true, err: "" } }));
    try {
      const data = await api<Quota>("GET", `/api/resources/${src}/${id}`);
      setQuotas((q) => ({ ...q, [k]: { data } }));
    } catch (e) {
      setQuotas((q) => ({ ...q, [k]: { ...q[k], loading: false, err: String((e as Error).message) } }));
    }
  }, []);

  const refreshAll = useCallback(async () => {
    setErr("");
    let list: Source[] = [];
    try {
      const all = (await api<{ sources: Source[] }>("GET", "/api/resources")).sources;
      // The same accounts copied to several 9routers share one quota: show them once (first reachable source wins).
      const seen = new Set<string>();
      list = all.map((s) => {
        // API-key accounts (e.g. OpenRouter) have no 5h / weekly quota to track.
        const accounts = s.error ? [] : s.accounts.filter((a) => a.authType !== "apikey" && !seen.has(a.id));
        const dup = s.accounts.length - accounts.length;
        accounts.forEach((a) => seen.add(a.id));
        return { ...s, accounts, dup };
      });
      setSources(list);
    } catch (e) {
      setErr(String((e as Error).message));
      setSources([]);
      return;
    }
    const jobs = list.flatMap((s) => s.accounts.map((a) => () => loadQuota(s.idx, a.id)));
    // A few at a time so 9router / Google are not flooded.
    for (let i = 0; i < jobs.length; i += 4) {
      await Promise.all(jobs.slice(i, i + 4).map((j) => j()));
    }
    setUpdated(Date.now());
  }, [loadQuota]);

  useEffect(() => {
    refreshAll();
  }, [refreshAll]);

  const total = (sources ?? []).reduce((n, s) => n + s.accounts.length, 0);

  return (
    <div className="panel-body resources">
      <section className="card">
        <div className="row between">
          <h3>Tài nguyên AI (9router)</h3>
          <button className="primary" onClick={refreshAll} disabled={!sources}>
            ↻ Làm mới
          </button>
        </div>
        <p className="muted small">
          {sources ? `${total} tài khoản · ${sources.map((s) => `${s.label}: ${s.error ? "lỗi kết nối" : s.accounts.length ? `${s.accounts.length} tài khoản` : s.dup ? "trùng tài khoản phía trên" : "trống"}`).join(" · ")}` : "Đang tải…"}
          {updated ? ` · cập nhật ${new Date(updated).toLocaleTimeString("vi-VN")}` : ""}
        </p>
        {err && <p className="error small">{err}</p>}
      </section>

      {(sources ?? []).filter((s) => s.error || s.accounts.length > 0).map((s) => (
        <section key={s.idx} className="res-source">
          <h3 className="res-src-title">
            {s.label} <span className="muted small">{s.url}</span>
          </h3>
          {s.error && <p className="error small card">{s.error}</p>}
          {groupByProvider(s.accounts).map(([provider, list]) => (
            <section key={provider} className="card">
              <h3 className="res-provider">
                {provider} <span className="muted small">({list.length})</span>
              </h3>
              <div className="res-list">
                {list.map((a) => (
                  <AccountCard key={a.id} a={a} q={quotas[key(s.idx, a.id)]} onRefresh={() => loadQuota(s.idx, a.id)} />
                ))}
              </div>
            </section>
          ))}
        </section>
      ))}
    </div>
  );
}

function AccountCard({ a, q, onRefresh }: { a: Account; q?: QState; onRefresh: () => void }) {
  const [open, setOpen] = useState(false);
  const st = status(a);
  const rows = q?.data?.quotas ?? [];
  // The tightest limit is what stops the account first.
  const limited = rows.filter((r) => !r.unlimited && typeof r.remainingPct === "number");
  const min = limited.length ? Math.min(...limited.map(pctOf)) : null;
  return (
    <div className={`res-acc ${open ? "open" : ""}`}>
      <div className="res-head" role="button" tabIndex={0} onClick={() => setOpen((o) => !o)} onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && setOpen((o) => !o)}>
        <span className="res-name clip" title={a.email || a.name}>
          {a.email || a.name || a.id}
        </span>
        {q?.data?.tier && <span className="pill blue">{q.data.tier}</span>}
        <span className={`pill ${st.cls}`} title={a.lastError || undefined}>
          {st.label}
        </span>
        <span className={`res-min ${min !== null ? tone(min) : "muted"}`}>{min !== null ? `${Math.round(min)}%` : q?.loading ? "…" : "–"}</span>
        <span className="muted">{open ? "▾" : "▸"}</span>
      </div>
      {min !== null && (
        <div className="minibar">
          <div className={`res-fill ${tone(min)}`} style={{ width: `${min}%` }} />
        </div>
      )}
      {open && (
        <div className="res-body">
      {q?.loading && !q.data && <p className="muted small">Đang tải…</p>}
      {q?.err && <p className="error small">{q.err}</p>}
      {q?.data?.message && <p className="muted small">{q.data.message === NO_QUOTA ? "Không có thông tin quota (tài khoản API key)." : q.data.message}</p>}
      {a.lastError && <p className="error small">Lỗi gần nhất: {a.lastError}</p>}
      {rows.map((r) => {
        const pct = pctOf(r);
        return (
          <div key={r.model} className="res-row">
            <span className="res-q" title={r.name}>
              {NAMES[r.model] ?? r.name}
            </span>
            <div className="res-meter">
              <div className="bar thin">
                <div className={`res-fill ${tone(pct)}`} style={{ width: `${pct}%` }} />
              </div>
              <div className="row between tiny">
                <span className="muted">{r.unlimited ? "không giới hạn" : `${fmt(r.used)} / ${fmt(r.total)}`}</span>
                <b className={`res-pct ${tone(pct)}`}>{Math.round(pct)}%</b>
              </div>
            </div>
            <span className="res-reset muted tiny">{resetIn(r.resetAt)}</span>
          </div>
        );
      })}
          <div className="row between small">
            <span />
            <button className="ghost" onClick={onRefresh} disabled={q?.loading}>
              ↻ Làm mới tài khoản này
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
