import { lazy, Suspense } from "react";
import { useEffect, useRef, useState } from "react";
import { api, uploadChatFiles } from "@/api/http";
import { useStore } from "@/store";
import { Markdown } from "@/shared/Markdown";
import { ROLE_LABEL } from "@/shared/labels";

export const MENU = [
  { id: "chat", label: "💬 Chat" },
  { id: "screen", label: "🖥 Xem màn hình" },
  { id: "profile", label: "👤 Xem hồ sơ" },
  { id: "close", label: "✖ Đóng" },
] as const;
export type MenuAction = (typeof MENU)[number]["id"];

/** What to do with the employee in reach: chosen with ↑ ↓ + Enter, the pad, or a tap. */
export function InteractMenu({
  empId,
  index,
  onPick,
}: {
  empId: string;
  index: number;
  onPick: (a: MenuAction) => void;
}) {
  const { state } = useStore();
  const e = state?.employees.find((x) => x.id === empId);
  const rt = state?.runtime[empId];
  if (!e) return null;
  return (
    <div
      className="interact-menu"
      role="menu"
      aria-label={`Tương tác với ${e.name}`}
    >
      <div className="im-head">
        <b>{e.name}</b> · {ROLE_LABEL[e.role]}
        <div className="small muted">{rt?.note || "Đang rảnh"}</div>
      </div>
      {MENU.map((m, i) => (
        <button
          key={m.id}
          role="menuitem"
          className={i === index ? "on" : ""}
          onClick={() => onPick(m.id)}
        >
          {i === index ? "▶ " : "  "}
          {m.label}
        </button>
      ))}
      <div className="small muted im-keys">
        Chạm để chọn · máy tính: ↑ ↓ Enter, Esc đóng
      </div>
    </div>
  );
}

/** Talking with an employee right in the office: they answer in a few seconds. */
/** The real terminal (xterm.js) is only downloaded when the maintainer's window opens. */
const Terminal = lazy(() =>
  import("@/features/maintainer/Terminal").then((m) => ({
    default: m.Terminal,
  })),
);

export function ChatPopup(props: {
  empId: string;
  onClose: () => void;
  embedded?: boolean;
}) {
  const { state } = useStore();
  // The system maintainer is a CLI session: shown as a terminal.
  if (state?.employees.find((x) => x.id === props.empId)?.role === "maintainer")
    return <Terminal {...props} />;
  return <PlainChat {...props} />;
}

function PlainChat({
  empId,
  onClose,
  embedded,
}: {
  empId: string;
  onClose: () => void;
  embedded?: boolean;
}) {
  const { state, selectedProject } = useStore();
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [err, setErr] = useState("");
  const [uploading, setUploading] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const e = state?.employees.find((x) => x.id === empId);
  const msgs = (state?.messages ?? [])
    .filter(
      (m) =>
        (m.from === "chairman" && m.to === empId) ||
        (m.from === empId && m.to === "chairman"),
    )
    .slice(-30);
  const replying = state?.runtime[empId]?.note === "Đang trả lời Chủ tịch";
  const last = msgs[msgs.length - 1];
  const waiting =
    replying || (last?.from === "chairman" && Date.now() - last.ts < 60_000);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);
  useEffect(() => {
    listRef.current?.scrollTo({ top: listRef.current.scrollHeight });
  }, [msgs.length, waiting]);

  if (!e) return null;
  const send = async () => {
    if (!text.trim() || sending) return;
    setSending(true);
    setErr("");
    try {
      await api("POST", "/api/chat", {
        to: empId,
        text: text.trim(),
        projectId: selectedProject ?? undefined,
      });
      setText("");
    } catch (x) {
      setErr((x as Error).message);
    } finally {
      setSending(false);
      inputRef.current?.focus();
    }
  };
  const onFiles = async (files: File[]) => {
    if (!files.length || uploading) return;
    setErr("");
    setUploading(true);
    try {
      const paths = await uploadChatFiles(files);
      if (paths.length) {
        const joined = paths.join(" ");
        setText((prev) => (prev ? `${prev.trimEnd()} ${joined} ` : `${joined} `));
        inputRef.current?.focus();
      }
    } catch (x) {
      setErr((x as Error).message || "Lỗi khi tải tệp");
    } finally {
      setUploading(false);
    }
  };
  return (
    <div
      className={`office-chat${embedded ? " embedded" : ""}`}
      role="dialog"
      aria-label={`Chat với ${e.name}`}
      onPaste={(ev) => {
        const files = Array.from(ev.clipboardData?.files ?? []);
        if (files.length) {
          ev.preventDefault();
          void onFiles(files);
        }
      }}
    >
      <input
        ref={fileRef}
        type="file"
        multiple
        hidden
        onChange={(ev) => {
          const files = Array.from(ev.target.files ?? []);
          ev.target.value = "";
          if (files.length) void onFiles(files);
        }}
      />
      <div className="row between oc-head">
        <b>
          💬 {e.name} · {ROLE_LABEL[e.role]}
        </b>
        <button className="ghost" onClick={onClose} aria-label="Đóng chat">
          ✖
        </button>
      </div>
      <div className="oc-list" ref={listRef}>
        {msgs.length === 0 && (
          <p className="muted small">
            Chưa có tin nhắn. Hãy chào {e.name} hoặc giao việc.
          </p>
        )}
        {msgs.map((m) => (
          <div
            key={m.id}
            className={`oc-msg ${m.from === "chairman" ? "me" : "them"}`}
          >
            <Markdown text={m.text} />
          </div>
        ))}
        {waiting && (
          <div className="oc-msg them typing">{e.name} đang trả lời…</div>
        )}
      </div>
      {err && <p className="warn small">{err}</p>}
      <div className="row oc-input">
        <div className="oc-box">
          <button
            type="button"
            className="oc-attach"
            disabled={uploading}
            onClick={() => fileRef.current?.click()}
            aria-label="Chọn tệp hoặc ảnh"
            title="Chọn một hoặc nhiều tệp/ảnh (hoặc Ctrl+V để dán)"
          >
            {uploading ? (
              "…"
            ) : (
              <svg width="18" height="18" viewBox="0 0 18 18" aria-hidden="true">
                <path d="M9 3.5v11M3.5 9h11" stroke="currentColor" strokeWidth="2.4" strokeLinecap="square" />
              </svg>
            )}
          </button>
          <textarea
            ref={inputRef}
            rows={2}
            placeholder={`Nhắn ${e.name}… (Enter gửi, Shift+Enter xuống dòng, Esc đóng)`}
            value={text}
            onChange={(ev) => setText(ev.target.value)}
            onKeyDown={(ev) => {
              if (ev.key === "Enter" && !ev.shiftKey) {
                ev.preventDefault();
                void send();
              } else if (ev.key === "Escape") {
                ev.preventDefault();
                onClose();
              }
            }}
          />
        </div>
        <button
          className="primary"
          disabled={!text.trim() || sending}
          onClick={() => void send()}
        >
          Gửi
        </button>
      </div>
    </div>
  );
}
