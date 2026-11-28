import { useEffect, useRef, useState } from "react";
import { api, uploadChatFiles } from "@/api/http";
import { useStore } from "@/store";
import { ROLE_LABEL } from "@/shared/labels";
import { time } from "@/shared/format";
import { Markdown } from "@/shared/Markdown";
import "./chat.css";

const KIND: Record<string, string> = {
  handoff: "Bàn giao",
  chat: "",
  report: "Báo cáo",
  system: "Hệ thống",
  question: "Câu hỏi",
};

export function ChatPanel() {
  const { state, selectedProject } = useStore();
  const [onlyProject, setOnlyProject] = useState(true);
  const [to, setTo] = useState("");
  const [text, setText] = useState("");
  const [uploading, setUploading] = useState(false);
  const [uploadErr, setUploadErr] = useState("");
  const endRef = useRef<HTMLDivElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const msgs = (state?.messages ?? []).filter((m) => !onlyProject || !selectedProject || m.projectId === selectedProject || !m.projectId);
  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [msgs.length]);
  if (!state) return null;

  const who = (id: string) => {
    if (id === "chairman") return { name: "Chủ tịch", color: "#ffd166", role: "" };
    if (id === "system") return { name: "Hệ thống", color: "#8a94a8", role: "" };
    if (id === "all") return { name: "Mọi người", color: "#8a94a8", role: "" };
    if (id === "customer") return { name: "Khách hàng", color: "#ffcd75", role: "" };
    const e = state.employees.find((x) => x.id === id);
    return e ? { name: e.name, color: e.color, role: ROLE_LABEL[e.role] } : { name: id, color: "#8a94a8", role: "" };
  };

  const send = async () => {
    const target = to || state.employees.find((e) => e.role === "ceo")?.id;
    if (!target || !text.trim()) return;
    await api("POST", "/api/chat", { to: target, text, projectId: selectedProject ?? undefined });
    setText("");
  };

  const onFiles = async (files: File[]) => {
    if (!files.length || uploading) return;
    setUploadErr("");
    setUploading(true);
    try {
      const paths = await uploadChatFiles(files);
      if (paths.length) {
        const joined = paths.join(" ");
        setText((prev) => (prev ? `${prev.trimEnd()} ${joined} ` : `${joined} `));
        inputRef.current?.focus();
      }
    } catch (err) {
      setUploadErr((err as Error).message || "Lỗi khi tải tệp");
    } finally {
      setUploading(false);
    }
  };

  return (
    <div className="chat">
      <input
        ref={fileRef}
        type="file"
        multiple
        hidden
        onChange={(e) => {
          const files = Array.from(e.target.files ?? []);
          e.target.value = "";
          if (files.length) void onFiles(files);
        }}
      />
      <label className="row small pad-x">
        <input type="checkbox" checked={onlyProject} onChange={(e) => setOnlyProject(e.target.checked)} /> Chỉ dự án đang chọn
      </label>
      <div className="msgs">
        {msgs.length === 0 && <p className="muted center">Chưa có tin nhắn.</p>}
        {msgs.map((m) => {
          const f = who(m.from);
          const t = who(m.to);
          return (
            <div key={m.id} className={`msg kind-${m.kind}`}>
              <div className="meta">
                <span className="dot" style={{ background: f.color }} />
                <b>{f.name}</b>
                {f.role && <span className="muted"> · {f.role}</span>}
                <span className="muted"> → {t.name}</span>
                {KIND[m.kind] && <span className="pill small-pill">{KIND[m.kind]}</span>}
                <span className="muted right">{time(m.ts)}</span>
              </div>
              <Markdown className="text" text={m.text} />
            </div>
          );
        })}
        <div ref={endRef} />
      </div>
      {uploadErr && <p className="warn small pad-x">{uploadErr}</p>}
      <div
        className="composer"
        onPaste={(e) => {
          const files = Array.from(e.clipboardData?.files ?? []);
          if (files.length) {
            e.preventDefault();
            void onFiles(files);
          }
        }}
      >
        <select value={to} onChange={(e) => setTo(e.target.value)}>
          <option value="">Gửi CEO</option>
          {state.employees.map((e) => (
            <option key={e.id} value={e.id}>
              {e.name} · {ROLE_LABEL[e.role]}
            </option>
          ))}
        </select>
        <button
          type="button"
          disabled={uploading}
          onClick={() => fileRef.current?.click()}
          title="Chọn một hoặc nhiều tệp/ảnh (hoặc Ctrl+V để dán)"
        >
          {uploading ? "…" : "Tệp"}
        </button>
        <input
          ref={inputRef}
          placeholder="Nhắn với tư cách Chủ tịch… (họ đọc ở lượt việc kế tiếp)"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && send()}
        />
        <button className="primary" onClick={send} disabled={!text.trim()}>
          Gửi
        </button>
      </div>
    </div>
  );
}
