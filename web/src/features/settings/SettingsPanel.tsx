import { useEffect, useState } from "react";
import { api } from "@/api/http";
import { useStore } from "@/store";
import { PROVIDER_LABEL, ROLE_LABEL } from "@/shared/labels";
import type { McpServer, ProviderId, Role, Settings } from "@/api/types";

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
      <p className="small muted">Bật / tắt công cụ cho nhân viên. Áp dụng từ lượt việc tiếp theo, không cần khởi động lại.</p>
      {err && <p className="small warn">{err}</p>}
      {!servers && !err && <p className="small muted">Đang tải…</p>}
      {servers?.length === 0 && <p className="small muted">Chưa có MCP nào trong mcp.json.</p>}
      <ul className="plain mcp-list">
        {servers?.map((s) => (
          <li key={s.name}>
            <label className="row">
              <input type="checkbox" checked={s.enabled} disabled={!s.available} onChange={(e) => toggle(s, e.target.checked)} />
              <b>{MCP_INFO[s.name]?.[0] ?? s.name}</b>
              <code className="small muted">{s.name}</code>
              {!s.available && <span className="small muted">(không chạy trên máy chủ này)</span>}
            </label>
            <p className="small muted">{MCP_INFO[s.name]?.[1] ?? s.hint.slice(0, 140)}</p>
            <p className="small">Dùng cho: {roles(s.roles)}</p>
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
  useEffect(() => {
    setDraft(state?.settings ?? null);
  }, [state?.settings]);
  if (!draft) return null;
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setDraft({ ...draft, [k]: v });
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
          Tự cho phép mọi lệnh (không hỏi Chủ tịch)
        </label>
        <p className="small warn">Chỉ bật khi bạn tin tưởng model: nhân viên có thể chạy lệnh tùy ý trong thư mục dự án.</p>
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
        <p className="small muted">Các trưởng bộ phận góp ý rồi phản biện bản nháp của CEO trước khi CEO chốt. 0 = CEO tự quyết. Mỗi vòng tốn thêm khoảng 12 lượt gọi model.</p>
        <label>
          Model dự phòng khi nhân viên gặp sự cố
          <input placeholder="vd. ag/gemini-4-hydrogen-tiered-high(xhigh)" value={draft.fallbackModel} onChange={(e) => set("fallbackModel", e.target.value)} />
        </label>
        <label>
          Model cho việc nhẹ (bỏ phiếu, biên bản, họp daily)
          <input placeholder="để trống = ai dùng model nấy" value={draft.lightModel} onChange={(e) => set("lightModel", e.target.value)} />
        </label>
        <label className="row">
          <input type="checkbox" checked={draft.approveDesign} onChange={(e) => set("approveDesign", e.target.checked)} />
          Chủ tịch duyệt bản thiết kế giao diện trước khi code
        </label>
        <label>
          Số lần tự chạy lại dự án khi lỗi (cách nhau 5 phút)
          <input type="number" min={0} max={10} value={draft.autoRetries} onChange={(e) => set("autoRetries", Number(e.target.value))} />
        </label>
        <p className="small muted">Khi một nhân viên gặp sự cố, cấp trên giao việc cho đồng nghiệp cùng vai (hoặc model dự phòng), rồi nếu vẫn lỗi thì diễn đạt lại yêu cầu rõ hơn và giao lại.</p>
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
        <p className="small muted">
          Khóa nằm trong file <code>.env</code> ở thư mục dự án (xem <code>.env.example</code>), không lưu trên giao diện. Sửa xong hãy khởi động lại backend.
        </p>
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
