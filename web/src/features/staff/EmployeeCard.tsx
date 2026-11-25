import { useEffect, useState } from "react";
import { api } from "@/api/http";
import { useStore } from "@/store";
import { confirmDialog } from "@/shared/ui/Dialog";
import { CLAUDE_MODELS, PROVIDER_ENV, PROVIDER_LABEL, ROLES, ROLE_LABEL, STATUS_LABEL } from "@/shared/labels";
import { num, usd } from "@/shared/format";
import type { Employee, ProviderId, Role } from "@/api/types";

const PROVIDERS = Object.keys(PROVIDER_LABEL) as ProviderId[];
const EFFORTS = ["", "low", "medium", "high", "xhigh", "max"];

export function EmployeeCard({ e, open, onToggle }: { e: Employee; open: boolean; onToggle: () => void }) {
  const { state, providers } = useStore();
  const [draft, setDraft] = useState(e);
  const [msg, setMsg] = useState("");
  useEffect(() => {
    setDraft(e);
  }, [e]);
  const rt = state!.runtime[e.id];
  const u = state!.usage[e.id];
  const dirty = JSON.stringify(draft) !== JSON.stringify(e);
  const set = <K extends keyof Employee>(k: K, v: Employee[K]) => setDraft((d) => ({ ...d, [k]: v }));

  const pickModel = (model: string) => {
    const known = CLAUDE_MODELS.find((m) => m.id === model);
    setDraft((d) => ({ ...d, model, ...(known && d.provider === "anthropic" ? { inputPrice: known.price[0], outputPrice: known.price[1] } : {}) }));
  };
  const save = async () => {
    setMsg("");
    try {
      await api("PUT", `/api/employees/${e.id}`, draft);
      setMsg("Đã lưu");
    } catch (err) {
      setMsg(String((err as Error).message));
    }
  };
  const fire = async () => {
    const ok = await confirmDialog({
      title: `Cho ${e.name} nghỉ việc?`,
      message: `${e.name} (${ROLE_LABEL[e.role]}) sẽ rời công ty. Lịch sử làm việc và chi phí vẫn được giữ trong báo cáo.`,
      okLabel: "Cho nghỉ",
      danger: true,
    });
    if (!ok) return;
    try {
      await api("DELETE", `/api/employees/${e.id}`);
    } catch (err) {
      setMsg(String((err as Error).message));
    }
  };
  const ready = providers[draft.provider];

  return (
    <section className={`card emp ${open ? "selected" : ""} ${e.enabled ? "" : "disabled"}`}>
      <div className="row between click" onClick={onToggle}>
        <div className="row gap6">
          <span className="avatar" style={{ background: e.color }}>
            {e.name.slice(0, 1)}
          </span>
          <div>
            <b>{e.name}</b> <span className="muted">· {ROLE_LABEL[e.role]}</span>
            <div className="small muted">{e.provider === "mock" ? "Mock" : `${PROVIDER_LABEL[e.provider].split(" ")[0]} · ${e.model}`}</div>
          </div>
        </div>
        <div className="right small">
          <span className={`status ${rt?.status ?? "idle"}`}>{STATUS_LABEL[rt?.status ?? "idle"]}</span>
          <div className="muted">{u ? usd(u.costUsd) : "$0"}</div>
        </div>
      </div>
      {open && (
        <div className="form">
          <label>
            Tên
            <input value={draft.name} onChange={(ev) => set("name", ev.target.value)} />
          </label>
          <label>
            Vai trò
            <select value={draft.role} onChange={(ev) => set("role", ev.target.value as Role)}>
              {ROLES.map((r) => (
                <option key={r} value={r}>
                  {ROLE_LABEL[r]}
                </option>
              ))}
            </select>
          </label>
          <label>
            Nhà cung cấp
            <select value={draft.provider} onChange={(ev) => set("provider", ev.target.value as ProviderId)}>
              {PROVIDERS.map((p) => (
                <option key={p} value={p}>
                  {PROVIDER_LABEL[p]} {providers[p] ? "" : "· chưa có khóa"}
                </option>
              ))}
            </select>
          </label>
          {!ready && PROVIDER_ENV[draft.provider] && (
            <p className="warn small">
              Thêm <code>{PROVIDER_ENV[draft.provider]}</code> vào file <code>.env</code> rồi khởi động lại backend.
            </p>
          )}
          {draft.provider === "custom" && (
            <label>
              Base URL
              <input placeholder="https://host/v1" value={draft.baseUrl} onChange={(ev) => set("baseUrl", ev.target.value)} />
            </label>
          )}
          <label>
            Model
            <input list={`models-${e.id}`} value={draft.model} onChange={(ev) => pickModel(ev.target.value)} />
            <datalist id={`models-${e.id}`}>
              {CLAUDE_MODELS.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                </option>
              ))}
            </datalist>
          </label>
          {draft.provider === "anthropic" && (
            <label>
              Effort (độ suy nghĩ)
              <select value={draft.effort} onChange={(ev) => set("effort", ev.target.value)}>
                {EFFORTS.map((x) => (
                  <option key={x} value={x}>
                    {x || "Mặc định của model"}
                  </option>
                ))}
              </select>
            </label>
          )}
          <div className="row">
            <label>
              Giá input $/1M
              <input type="number" step="0.01" min={0} value={draft.inputPrice} onChange={(ev) => set("inputPrice", Number(ev.target.value))} />
            </label>
            <label>
              Giá output $/1M
              <input type="number" step="0.01" min={0} value={draft.outputPrice} onChange={(ev) => set("outputPrice", Number(ev.target.value))} />
            </label>
          </div>
          <label>
            Chỉ dẫn riêng (thêm vào system prompt)
            <textarea rows={3} value={draft.extraPrompt} onChange={(ev) => set("extraPrompt", ev.target.value)} />
          </label>
          <label className="row">
            <input type="checkbox" checked={draft.enabled} onChange={(ev) => set("enabled", ev.target.checked)} /> Đang làm việc
          </label>
          {u && (
            <p className="small muted">
              {num(u.calls)} lượt gọi · {num(u.inputTokens)} token vào · {num(u.outputTokens)} token ra · {usd(u.costUsd)}
            </p>
          )}
          <div className="row">
            <button className="primary" disabled={!dirty} onClick={save}>
              Lưu
            </button>
            <button className="ghost" disabled={!dirty} onClick={() => setDraft(e)}>
              Hủy
            </button>
            <button className="ghost danger right" onClick={fire}>
              Cho nghỉ
            </button>
          </div>
          {msg && <p className="small muted">{msg}</p>}
        </div>
      )}
    </section>
  );
}
