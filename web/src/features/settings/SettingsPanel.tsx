import { useEffect, useRef, useState } from "react";
import { api } from "@/api/http";
import { useStore } from "@/store";
import { PROVIDER_LABEL, ROLE_LABEL } from "@/shared/labels";
import type { McpServer, ProviderId, Role, Settings } from "@/api/types";

interface ToolsSkillInfo {
  exists: boolean;
  name: string;
  path: string;
  chars: number;
  content: string;
}

const MCP_INFO: Record<string, [string, string]> = {
  browser: ["Trình duyệt", "Mở bản xem thử, bấm, gõ, chụp màn hình và đọc lỗi như người dùng thật."],
  desktop: ["Điều khiển máy tính", "Mở và thao tác ứng dụng Windows / giả lập để test thật. Chỉ chạy trên Windows."],
  docs: ["Tài liệu thư viện", "Tra tài liệu mới nhất của thư viện trước khi viết code."],
  memory: ["Bộ nhớ công ty", "Ghi nhớ bài học và kinh nghiệm qua các dự án."],
  thinking: ["Suy nghĩ từng bước", "Ghi lại từng bước suy nghĩ cho vấn đề khó. Tốn thêm lượt gọi model."],
};

/** MCP servers from mcp.json, each switched on/off immediately (applies from the next job). */
function McpSection() {
  const { state } = useStore();
  const [servers, setServers] = useState<McpServer[] | null>(null);
  const [err, setErr] = useState("");
  const load = () =>
    api<{ servers: McpServer[] }>("GET", "/api/mcp")
      .then((r) => setServers(r.servers))
      .catch((e) => setErr(String((e as Error).message)));
  useEffect(() => {
    load();
  }, []);
  const toggle = async (s: McpServer, on: boolean) => {
    if (!state) return;
    const mcp = { ...(state.settings.mcp ?? {}) };
    if (on === s.defaultEnabled) delete mcp[s.name];
    else mcp[s.name] = on;
    setServers((list) => list?.map((x) => (x.name === s.name ? { ...x, enabled: on } : x)) ?? null);
    try {
      await api("PUT", "/api/settings", { ...state.settings, mcp });
      setErr("");
    } catch (e) {
      setErr(String((e as Error).message));
      load();
    }
  };
  const roles = (r: string[]) => (r.length ? r.map((k) => ROLE_LABEL[k as Role] ?? k).join(", ") : "Mọi nhân viên");

  return (
    <section className="card">
      <h3>Công cụ MCP</h3>
      {err && <p className="small warn">{err}</p>}
      {!servers && !err && <p className="small muted">Đang tải…</p>}
      {servers?.length === 0 && <p className="small muted">Chưa có MCP nào trong mcp.json.</p>}
      <ul className="plain mcp-list">
        {servers?.map((s) => (
          <li key={s.name}>
            <label className="row" title={MCP_INFO[s.name]?.[1] ?? s.hint}>
              <input type="checkbox" checked={s.enabled} disabled={!s.available} onChange={(e) => toggle(s, e.target.checked)} />
              <b>{MCP_INFO[s.name]?.[0] ?? s.name}</b>
              <code className="small muted">{s.name}</code>
              {!s.available && <span className="small muted">(không khả dụng)</span>}
            </label>
            <p className="small muted">{roles(s.roles)}</p>
          </li>
        ))}
      </ul>
    </section>
  );
}

export function SettingsPanel() {
  const { state, providers } = useStore();
  const [draft, setDraft] = useState<Settings | null>(state?.settings ?? null);
  const [msg, setMsg] = useState("");
  const [toolsFile, setToolsFile] = useState<ToolsSkillInfo | null>(null);
  const [showFile, setShowFile] = useState(false);
  const [fileMsg, setFileMsg] = useState("");
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  useEffect(() => {
    setDraft(state?.settings ?? null);
  }, [state?.settings]);
  const loadToolsFile = () => {
    api<ToolsSkillInfo>("GET", "/api/tools-skill")
      .then((info) => setToolsFile(info))
      .catch(() => setToolsFile(null));
  };
  useEffect(() => {
    loadToolsFile();
  }, [state?.settings.toolsSkillFile]);
  if (!draft) return null;
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setDraft({ ...draft, [k]: v });
  const uploadToolsFile = async (file: File | undefined) => {
    if (!file) return;
    setFileMsg("");
    try {
      const content = await file.text();
      const info = await api<ToolsSkillInfo>("PUT", "/api/tools-skill", { name: file.name, content });
      setToolsFile(info);
      setDraft((d) => (d ? { ...d, toolsSkillFile: info.path } : d));
      setFileMsg(`Đã thêm tệp ${info.name}`);
    } catch (e) {
      setFileMsg(String((e as Error).message));
    }
  };
  const removeToolsFile = async () => {
    setFileMsg("");
    try {
      const info = await api<ToolsSkillInfo>("DELETE", "/api/tools-skill");
      setToolsFile(info);
      setShowFile(false);
      setDraft((d) => (d ? { ...d, toolsMode: false, toolsSkillFile: "" } : d));
      setFileMsg("Đã xóa tệp");
    } catch (e) {
      setFileMsg(String((e as Error).message));
    }
  };
  const save = async () => {
    try {
      // MCP switches save themselves; keep the live ones rather than this draft's copy.
      await api("PUT", "/api/settings", { ...draft, mcp: state?.settings.mcp ?? {} });
      setMsg("Đã lưu");
    } catch (e) {
      setMsg(String((e as Error).message));
    }
  };

  return (
    <div className="panel-body">
      <section className="card form">
        <h3>Vận hành</h3>
        <label className="row">
          <input type="checkbox" checked={draft.autoApproveCommands} onChange={(e) => set("autoApproveCommands", e.target.checked)} />
          Tự cho phép mọi lệnh
        </label>
        <label>
          Số lượt gọi model cùng lúc
          <input type="number" min={1} max={16} value={draft.maxConcurrent} onChange={(e) => set("maxConcurrent", Number(e.target.value))} />
        </label>
        <label>
          Số lần trả ticket tối đa trước khi chặn
          <input type="number" min={0} max={10} value={draft.maxBounces} onChange={(e) => set("maxBounces", Number(e.target.value))} />
        </label>
        <label>
          Số bước tối đa cho một lượt việc
          <input type="number" min={3} max={200} value={draft.maxAgentSteps} onChange={(e) => set("maxAgentSteps", Number(e.target.value))} />
        </label>
        <label>
          Ngân sách mặc định mỗi dự án (USD)
          <input type="number" min={0} step={0.5} value={draft.defaultBudgetUsd} onChange={(e) => set("defaultBudgetUsd", Number(e.target.value))} />
        </label>
        <label>
          Số vòng thảo luận ở họp khởi động
          <input type="number" min={0} max={5} value={draft.meetingRounds} onChange={(e) => set("meetingRounds", Number(e.target.value))} />
        </label>
        <label>
          Model dự phòng
          <input placeholder="vd. ag/gemini-4-hydrogen-tiered-high(xhigh)" value={draft.fallbackModel} onChange={(e) => set("fallbackModel", e.target.value)} />
        </label>
        <label>
          Model cho việc nhẹ
          <input placeholder="để trống = dùng model nhân viên" value={draft.lightModel} onChange={(e) => set("lightModel", e.target.value)} />
        </label>
        <label className="row">
          <input type="checkbox" checked={draft.approveDesign} onChange={(e) => set("approveDesign", e.target.checked)} />
          Chủ tịch duyệt bản thiết kế giao diện trước khi code
        </label>
        <label className="row">
          <input type="checkbox" checked={Boolean(draft.toolsMode)} onChange={(e) => set("toolsMode", e.target.checked)} />
          Chế độ làm tools
        </label>
        <div className="row" style={{ gap: 6, flexWrap: "wrap", alignItems: "center" }}>
          <input
            ref={fileInputRef}
            type="file"
            accept=".md,.markdown,.txt"
            style={{ display: "none" }}
            onChange={(e) => {
              uploadToolsFile(e.target.files?.[0]);
              e.target.value = "";
            }}
          />
          <button type="button" onClick={() => fileInputRef.current?.click()}>
            Thêm tệp
          </button>
          {toolsFile?.exists && (
            <>
              <button type="button" onClick={() => setShowFile((v) => !v)}>
                {showFile ? "Ẩn tệp" : "Xem tệp"}
              </button>
              <button type="button" className="danger" onClick={removeToolsFile}>
                Xóa tệp
              </button>
              <span className="small muted">
                {toolsFile.name} ({toolsFile.chars.toLocaleString("vi-VN")} ký tự)
              </span>
            </>
          )}
          {!toolsFile?.exists && <span className="small muted">Chưa có tệp</span>}
        </div>
        {fileMsg && <p className="small muted">{fileMsg}</p>}
        {showFile && toolsFile?.exists && (
          <textarea
            readOnly
            rows={12}
            value={toolsFile.content}
            style={{ width: "100%", fontFamily: "monospace", fontSize: 12 }}
          />
        )}
        <label>
          Số lần tự chạy lại khi lỗi
          <input type="number" min={0} max={10} value={draft.autoRetries} onChange={(e) => set("autoRetries", Number(e.target.value))} />
        </label>
        <label>
          Độ trễ giả lập của Mock (ms)
          <input type="number" min={0} max={10000} step={100} value={draft.mockDelayMs} onChange={(e) => set("mockDelayMs", Number(e.target.value))} />
        </label>
        <div className="row">
          <button className="primary" onClick={save}>
            Lưu
          </button>
          {msg && <span className="small muted">{msg}</span>}
        </div>
      </section>
      <McpSection />
      <section className="card">
        <h3>Khóa API</h3>
        <ul className="plain">
          {(Object.keys(PROVIDER_LABEL) as ProviderId[]).map((p) => (
            <li key={p} className="row between small">
              <span>{PROVIDER_LABEL[p]}</span>
              <span className={providers[p] ? "ok" : "muted"}>{providers[p] ? "● sẵn sàng" : "○ chưa có khóa"}</span>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
