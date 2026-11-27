import { useCallback, useEffect, useRef, useState } from "react";
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

interface XoayLog {
  at: string;
  level: "ok" | "warn" | "err" | "info" | string;
  msg: string;
}

interface XoayStatus {
  configured: boolean;
  url?: string;
  error?: string;
  running?: boolean;
  selected_model?: string;
  force_selected_model?: boolean;
  reasoning_effort?: string;
  force_reasoning_effort?: boolean;
  prompt_mode?: string;
  models?: string[];
  interval_secs?: number;
  min_interval_secs?: number;
  max_concurrent_per_exit?: number;
  current_proxy?: string;
  exit_ip?: string | null;
  latency_ms?: number | null;
  health_error?: string | null;
  rotating?: boolean;
  in_flight?: number;
  queued?: number;
  daemon_ready?: boolean;
  keepalive_hits?: number;
  keepalive_misses?: number;
  next_rotate_in?: number;
  api_expires_in?: number;
  requests_ok?: number;
  requests_failed?: number;
  rotations?: number;
  quota_hits?: number;
  logs?: XoayLog[];
}

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
      <XoaySection />

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

function XoaySection() {
  const [st, setSt] = useState<XoayStatus | null>(null);
  const [busy, setBusy] = useState("");
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");
  const [autoPoll, setAutoPoll] = useState(true);
  const [showCfg, setShowCfg] = useState(true);
  const [showLogs, setShowLogs] = useState(true);

  // Local editable form fields (synced once from server or after save, never clobbered mid-edit).
  const [dirty, setDirty] = useState(false);
  const [model, setModel] = useState("opencode/deepseek-v4.1-flash-free");
  const [forceModel, setForceModel] = useState(true);
  const [effort, setEffort] = useState("low");
  const [forceEffort, setForceEffort] = useState(false);
  const [promptMode, setPromptMode] = useState("clean");
  const [maxConc, setMaxConc] = useState(4);
  const [intervalSecs, setIntervalSecs] = useState(1800);
  const logBoxRef = useRef<HTMLDivElement>(null);

  const syncForm = useCallback((d: XoayStatus) => {
    if (d.selected_model !== undefined) setModel(d.selected_model);
    if (d.force_selected_model !== undefined) setForceModel(Boolean(d.force_selected_model));
    if (d.reasoning_effort !== undefined) setEffort(d.reasoning_effort || "");
    if (d.force_reasoning_effort !== undefined) setForceEffort(Boolean(d.force_reasoning_effort));
    if (d.prompt_mode !== undefined) setPromptMode(d.prompt_mode || "clean");
    if (d.max_concurrent_per_exit !== undefined) setMaxConc(d.max_concurrent_per_exit || 4);
    if (d.interval_secs !== undefined) setIntervalSecs(d.interval_secs || 1800);
  }, []);

  const fetchStatus = useCallback(
    async (forceSync = false) => {
      try {
        const d = await api<XoayStatus>("GET", "/api/xoay");
        setSt(d);
        setErr(d.error || "");
        if (d.configured && !d.error && (forceSync || !dirty)) {
          syncForm(d);
        }
      } catch (e) {
        setErr(String((e as Error).message));
      }
    },
    [dirty, syncForm],
  );

  useEffect(() => {
    fetchStatus(true);
  }, [fetchStatus]);

  useEffect(() => {
    if (!autoPoll) return;
    const t = setInterval(() => {
      void fetchStatus(false);
    }, 3000);
    return () => clearInterval(t);
  }, [autoPoll, fetchStatus]);

  // Keep log box scrolled to the newest line when user is near the bottom.
  useEffect(() => {
    const el = logBoxRef.current;
    if (!el) return;
    const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
    if (nearBottom) el.scrollTop = el.scrollHeight;
  }, [st?.logs?.length]);

  const flash = (text: string) => {
    setMsg(text);
    setTimeout(() => setMsg((cur) => (cur === text ? "" : cur)), 2600);
  };

  const saveConfig = async (overrideRunning?: boolean) => {
    setBusy("save");
    setErr("");
    try {
      const payload: Record<string, unknown> = {
        selected_model: model.trim(),
        force_selected_model: forceModel,
        reasoning_effort: effort,
        force_reasoning_effort: forceEffort,
        prompt_mode: promptMode,
        max_concurrent_per_exit: Math.max(1, Math.min(12, Number(maxConc) || 4)),
        interval_secs: Math.max(60, Math.min(86400, Number(intervalSecs) || 1800)),
      };
      if (overrideRunning !== undefined) payload.running = overrideRunning;
      const d = await api<XoayStatus>("POST", "/api/xoay/config", payload);
      setSt(d);
      setDirty(false);
      syncForm(d);
      flash(overrideRunning === undefined ? "Đã lưu cấu hình Xoay" : overrideRunning ? "Đã bật Xoay" : "Đã dừng Xoay");
    } catch (e) {
      setErr(String((e as Error).message));
    } finally {
      setBusy("");
    }
  };

  const runAction = async (action: "rotate" | "check" | "refresh-models", label: string) => {
    setBusy(action);
    setErr("");
    try {
      const d = await api<XoayStatus>("POST", `/api/xoay/${action}`, {});
      setSt(d);
      if (action === "refresh-models" && !dirty) syncForm(d);
      flash(label);
    } catch (e) {
      setErr(String((e as Error).message));
    } finally {
      setBusy("");
    }
  };

  if (st && !st.configured) return null;

  const models = st?.models?.length
    ? st.models.includes(model)
      ? st.models
      : [model, ...st.models]
    : [model];
  const logs = st?.logs ?? [];
  const pillCls = st?.rotating ? "amber" : st?.running ? "green" : "red";
  const pillTxt = st?.rotating ? "Đang xoay…" : st?.running ? "Đang chạy" : "Đang dừng";

  return (
    <section className="card xoay-card">
      <div className="row between wrap">
        <div className="row gap6">
          <h3>Xoay OpenCode Proxy</h3>
          {st && !st.error && <span className={`pill ${pillCls}`}>{pillTxt}</span>}
        </div>
        <div className="row gap4">
          <button className="ghost small" onClick={() => fetchStatus(true)} disabled={Boolean(busy)} title="Tải lại">
            ↻
          </button>
          <button className="ghost small" onClick={() => runAction("check", "Đã kiểm tra IP")} disabled={Boolean(busy)}>
            {busy === "check" ? "Đang kiểm…" : "Kiểm IP"}
          </button>
          <button className="primary small" onClick={() => runAction("rotate", "Đã xoay IP mới")} disabled={Boolean(busy)}>
            {busy === "rotate" ? "Đang xoay…" : "Xoay IP"}
          </button>
        </div>
      </div>

      {!st && <p className="muted small">Đang kết nối Xoay Proxy…</p>}
      {err && <p className="error small">{err}</p>}

      {st && !st.error && (
        <>
          <div className="kpi-grid">
            <div>
              <b>{fmt(st.requests_ok ?? 0)}</b>
              <span>Request OK</span>
            </div>
            <div>
              <b style={{ color: "var(--text)" }}>{fmt(st.rotations ?? 0)}</b>
              <span>Lượt xoay</span>
            </div>
            <div>
              <b style={{ color: "var(--amber)" }}>{fmt(st.quota_hits ?? 0)}</b>
              <span>Hết quota</span>
            </div>
            <div>
              <b style={{ color: (st.requests_failed ?? 0) > 0 ? "var(--red)" : "var(--muted)" }}>{fmt(st.requests_failed ?? 0)}</b>
              <span>Lỗi</span>
            </div>
          </div>

          <div className="xoay-meta small muted">
            <span>
              IP: <b className="ok mono">{st.exit_ip || "chưa rõ"}</b>
              {st.latency_ms ? ` · ${st.latency_ms}ms` : ""}
            </span>
            <span>
              ⚡KA: <b className={st.daemon_ready ? "ok" : "warn"}>{st.daemon_ready ? `${st.keepalive_hits ?? 0}/${st.keepalive_misses ?? 0}` : "tắt"}</b>
            </span>
            <span>
              Luồng: <b>{st.in_flight ?? 0}/{st.max_concurrent_per_exit ?? 4}</b>
              {(st.queued ?? 0) > 0 ? ` · đợi ${st.queued}` : ""}
            </span>
            {Boolean(st.api_expires_in) && <span>Hết hạn IP: {st.api_expires_in}s</span>}
            {Boolean(st.next_rotate_in) && <span>Xoay kế: {st.next_rotate_in}s</span>}
          </div>

          <div className="xoay-subhead row between">
            <button className="ghost small" onClick={() => setShowCfg((v) => !v)}>
              {showCfg ? "▾" : "▸"} Model, suy nghĩ &amp; chế độ
            </button>
            <div className="row gap4">
              {msg && <span className="ok small">{msg}</span>}
              <button className={st.running ? "danger small" : "primary small"} onClick={() => saveConfig(!st.running)} disabled={Boolean(busy)}>
                {st.running ? "Dừng" : "Bắt đầu"}
              </button>
            </div>
          </div>

          {showCfg && (
            <div className="form xoay-form">
              <label>
                Model OpenCode
                <div className="row gap4">
                  <select
                    value={model}
                    onChange={(e) => {
                      setModel(e.target.value);
                      setDirty(true);
                    }}
                  >
                    {models.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    className="ghost small"
                    onClick={() => runAction("refresh-models", "Đã tải danh sách model")}
                    disabled={Boolean(busy)}
                    title="Tải lại danh sách model miễn phí"
                  >
                    ↻
                  </button>
                </div>
              </label>

              <label className="row small">
                <input
                  type="checkbox"
                  checked={forceModel}
                  onChange={(e) => {
                    setForceModel(e.target.checked);
                    setDirty(true);
                  }}
                />
                <span>Ép dùng model này (vẫn giữ hậu tố #none/#low/#max)</span>
              </label>

              <label>
                Mức suy nghĩ (Reasoning Effort)
                <select
                  value={effort}
                  onChange={(e) => {
                    setEffort(e.target.value);
                    setDirty(true);
                  }}
                >
                  <option value="">Tự động (theo app gửi)</option>
                  <option value="none">none — tắt suy nghĩ (nhanh nhất)</option>
                  <option value="low">low — suy nghĩ nhẹ (khuyến nghị)</option>
                  <option value="medium">medium — cân bằng</option>
                  <option value="high">high — suy nghĩ sâu</option>
                  <option value="max">max — sâu nhất (chậm)</option>
                </select>
              </label>

              <label className="row small">
                <input
                  type="checkbox"
                  checked={forceEffort}
                  onChange={(e) => {
                    setForceEffort(e.target.checked);
                    setDirty(true);
                  }}
                />
                <span>Ép dùng mức suy nghĩ này cho mọi request</span>
              </label>

              <label>
                Lời nhắc hệ thống (System Prompt)
                <select
                  value={promptMode}
                  onChange={(e) => {
                    setPromptMode(e.target.value);
                    setDirty(true);
                  }}
                >
                  <option value="clean">clean — Sạch (không chèn lời nhắc OpenCode)</option>
                  <option value="full">full — Đầy đủ chuẩn OpenCode CLI</option>
                  <option value="raw">raw — Nguyên bản 100%</option>
                </select>
              </label>

              <div className="row gap6">
                <label>
                  Luồng / IP
                  <input
                    type="number"
                    min={1}
                    max={12}
                    value={maxConc}
                    onChange={(e) => {
                      setMaxConc(Number(e.target.value) || 4);
                      setDirty(true);
                    }}
                  />
                </label>
                <label>
                  Chu kỳ xoay (giây)
                  <input
                    type="number"
                    min={60}
                    max={86400}
                    value={intervalSecs}
                    onChange={(e) => {
                      setIntervalSecs(Number(e.target.value) || 1800);
                      setDirty(true);
                    }}
                  />
                </label>
              </div>

              <div className="row between">
                <span className="muted tiny">{dirty ? "Có thay đổi chưa lưu" : st.url}</span>
                <button className="primary" onClick={() => saveConfig()} disabled={Boolean(busy)}>
                  {busy === "save" ? "Đang lưu…" : "Lưu cấu hình"}
                </button>
              </div>
            </div>
          )}

          <div className="xoay-subhead row between">
            <button className="ghost small" onClick={() => setShowLogs((v) => !v)}>
              {showLogs ? "▾" : "▸"} Nhật ký hoạt động ({logs.length})
            </button>
            <label className="row small muted" style={{ cursor: "pointer" }}>
              <input type="checkbox" checked={autoPoll} onChange={(e) => setAutoPoll(e.target.checked)} />
              <span>Tự cập nhật 3s</span>
            </label>
          </div>

          {showLogs && (
            <div className="xoay-logs" ref={logBoxRef}>
              {logs.length === 0 && <div className="muted">Chưa có nhật ký.</div>}
              {logs.map((l, i) => (
                <div key={`${l.at}-${i}`} className="xoay-log-line">
                  <span className="muted">{l.at}</span>{" "}
                  <span className={`xoay-lvl ${l.level}`}>[{l.level.toUpperCase()}]</span>{" "}
                  <span>{l.msg}</span>
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </section>
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
