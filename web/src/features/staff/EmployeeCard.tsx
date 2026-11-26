import { useEffect, useRef, useState } from "react";
import { api } from "@/api/http";
import { useStore } from "@/store";
import { confirmDialog } from "@/shared/ui/Dialog";
import { CLAUDE_MODELS, PROVIDER_ENV, PROVIDER_LABEL, ROLES, ROLE_LABEL, STATUS_LABEL } from "@/shared/labels";
import { num, usd } from "@/shared/format";
import type { Employee, ProviderId, Role } from "@/api/types";

const PROVIDERS = Object.keys(PROVIDER_LABEL) as ProviderId[];
const EFFORTS = ["", "low", "medium", "high", "xhigh", "max"];
const MAX_SKILL_CHARS = 40_000;

interface SkillPreset {
  name: string;
  path: string;
  content: string;
}

function normalize(e: Employee): Employee {
  return { ...e, skillText: e.skillText ?? "", skillFile: e.skillFile ?? "" };
}

export function EmployeeCard({ e, open, onToggle }: { e: Employee; open: boolean; onToggle: () => void }) {
  const { state, providers } = useStore();
  const base = normalize(e);
  const [draft, setDraft] = useState<Employee>(base);
  const [msg, setMsg] = useState("");
  const [skillMsg, setSkillMsg] = useState("");
  const [dragOver, setDragOver] = useState(false);
  const [presets, setPresets] = useState<SkillPreset[]>([]);
  const fileRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    setDraft(normalize(e));
  }, [e]);

  useEffect(() => {
    if (!open || presets.length > 0) return;
    let alive = true;
    api<{ files: SkillPreset[] }>("GET", "/api/skills")
      .then((res) => {
        if (alive && Array.isArray(res?.files)) setPresets(res.files);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [open, presets.length]);

  const rt = state!.runtime[e.id];
  const u = state!.usage[e.id];
  const dirty = JSON.stringify(draft) !== JSON.stringify(base);
  const hasSkill = Boolean(base.skillText?.trim() || base.skillFile?.trim());
  const set = <K extends keyof Employee>(k: K, v: Employee[K]) => setDraft((d) => ({ ...d, [k]: v }));

  const appendSkill = (text: string, sourceLabel: string) => {
    const clean = text.trim();
    if (!clean) return;
    setDraft((d) => {
      const cur = (d.skillText ?? "").trim();
      const next = cur ? `${cur}\n\n---\n\n${clean}` : clean;
      return { ...d, skillText: next.slice(0, MAX_SKILL_CHARS) };
    });
    setSkillMsg(`Đã nạp ${sourceLabel} — bấm Lưu để áp dụng.`);
  };

  const importFiles = async (files: FileList | null) => {
    if (!files || files.length === 0) return;
    setSkillMsg("");
    const valid = Array.from(files).filter((f) => /\.(md|markdown|txt)$/i.test(f.name));
    if (valid.length === 0) {
      setSkillMsg("Chỉ hỗ trợ tệp .md, .markdown hoặc .txt");
      return;
    }
    try {
      const parts = await Promise.all(
        valid.map(async (f) => {
          const text = (await f.text()).trim();
          return valid.length > 1 ? `<!-- ${f.name} -->\n${text}` : text;
        }),
      );
      const merged = parts.filter(Boolean).join("\n\n---\n\n");
      if (!merged) {
        setSkillMsg("Tệp đã chọn không có nội dung.");
        return;
      }
      appendSkill(merged, valid.map((f) => f.name).join(", "));
    } catch (err) {
      setSkillMsg(`Không đọc được tệp: ${(err as Error).message}`);
    } finally {
      if (fileRef.current) fileRef.current.value = "";
    }
  };

  const pickModel = (model: string) => {
    const known = CLAUDE_MODELS.find((m) => m.id === model);
    setDraft((d) => ({ ...d, model, ...(known && d.provider === "anthropic" ? { inputPrice: known.price[0], outputPrice: known.price[1] } : {}) }));
  };
  const save = async () => {
    setMsg("");
    try {
      await api("PUT", `/api/employees/${e.id}`, draft);
      setSkillMsg("");
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
  const skillLen = (draft.skillText ?? "").length;

  return (
    <section className={`card emp ${open ? "selected" : ""} ${e.enabled ? "" : "disabled"}`}>
      <div className="row between click" onClick={onToggle}>
        <div className="row gap6">
          <span className="avatar" style={{ background: e.color }}>
            {e.name.slice(0, 1)}
          </span>
          <div>
            <b>{e.name}</b> <span className="muted">· {ROLE_LABEL[e.role]}</span>{" "}
            {hasSkill && <span className="pill green">Skill</span>}
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
          <div
            className={`skill-box ${dragOver ? "drag-over" : ""}`}
            onDragOver={(ev) => {
              ev.preventDefault();
              setDragOver(true);
            }}
            onDragLeave={() => setDragOver(false)}
            onDrop={(ev) => {
              ev.preventDefault();
              setDragOver(false);
              void importFiles(ev.dataTransfer.files);
            }}
          >
            <div className="row between wrap gap4">
              <span className="skill-title">Skill riêng (.md hoặc điền text)</span>
              <div className="row gap4 wrap">
                <input
                  ref={fileRef}
                  type="file"
                  accept=".md,.markdown,.txt"
                  multiple
                  hidden
                  onChange={(ev) => {
                    void importFiles(ev.target.files);
                  }}
                />
                <button type="button" className="ghost small" onClick={() => fileRef.current?.click()}>
                  Nạp tệp .md
                </button>
                {presets.length > 0 && (
                  <select
                    className="skill-preset"
                    value=""
                    aria-label="Nạp mẫu từ thư mục skills"
                    onChange={(ev) => {
                      const picked = presets.find((p) => p.path === ev.target.value);
                      if (picked) appendSkill(picked.content, picked.name);
                    }}
                  >
                    <option value="">+ Mẫu skills/</option>
                    {presets.map((p) => (
                      <option key={p.path} value={p.path}>
                        {p.name}
                      </option>
                    ))}
                  </select>
                )}
                {(skillLen > 0 || (draft.skillFile ?? "").trim()) && (
                  <button
                    type="button"
                    className="ghost small danger"
                    onClick={() => {
                      setDraft((d) => ({ ...d, skillText: "", skillFile: "" }));
                      setSkillMsg("Đã xóa skill — bấm Lưu để áp dụng.");
                    }}
                  >
                    Xóa
                  </button>
                )}
              </div>
            </div>
            <textarea
              rows={5}
              className="skill-textarea"
              placeholder="Gõ / dán nội dung skill Markdown, hoặc bấm 'Nạp tệp .md' / kéo thả tệp .md vào đây…"
              value={draft.skillText ?? ""}
              onChange={(ev) => set("skillText", ev.target.value)}
            />
            <div className="row between small muted">
              <span>{skillMsg || "Áp dụng cùng skill mặc định của vai trò khi nhân viên nhận việc."}</span>
              <span>
                {num(skillLen)} / {num(MAX_SKILL_CHARS)}
              </span>
            </div>
            <label>
              Hoặc trỏ đường dẫn tệp .md (tự đọc bản mới nhất mỗi lần chạy)
              <input
                list={`skill-files-${e.id}`}
                placeholder="vd: skills/dev.md hoặc C:\path\to\SKILL.md"
                value={draft.skillFile ?? ""}
                onChange={(ev) => set("skillFile", ev.target.value)}
              />
              <datalist id={`skill-files-${e.id}`}>
                {presets.map((p) => (
                  <option key={p.path} value={p.path}>
                    {p.name}
                  </option>
                ))}
              </datalist>
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
            <button
              className="ghost"
              disabled={!dirty}
              onClick={() => {
                setDraft(base);
                setSkillMsg("");
              }}
            >
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
