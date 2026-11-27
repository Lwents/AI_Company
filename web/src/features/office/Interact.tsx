import { useEffect, useRef, useState } from "react";
import { api } from "@/api/http";
import { useStore } from "@/store";
import { Markdown } from "@/shared/Markdown";
import { ROLE_LABEL } from "@/shared/labels";
import type { Keys } from "./engine/scene";

export const MENU = [
  { id: "chat", label: "💬 Chat" },
  { id: "screen", label: "🖥 Xem màn hình" },
  { id: "profile", label: "👤 Xem hồ sơ" },
  { id: "close", label: "✖ Đóng" },
] as const;
export type MenuAction = (typeof MENU)[number]["id"];

/** What to do with the employee in reach: chosen with ↑ ↓ + Enter, the pad, or a tap. */
export function InteractMenu({ empId, index, onPick }: { empId: string; index: number; onPick: (a: MenuAction) => void }) {
  const { state } = useStore();
  const e = state?.employees.find((x) => x.id === empId);
  const rt = state?.runtime[empId];
  if (!e) return null;
  return (
    <div className="interact-menu" role="menu" aria-label={`Tương tác với ${e.name}`}>
      <div className="im-head">
        <b>{e.name}</b> · {ROLE_LABEL[e.role]}
        <div className="small muted">{rt?.note || "Đang rảnh"}</div>
      </div>
      {MENU.map((m, i) => (
        <button key={m.id} role="menuitem" className={i === index ? "on" : ""} onClick={() => onPick(m.id)}>
          {i === index ? "▶ " : "  "}
          {m.label}
        </button>
      ))}
      <div className="small muted im-keys">↑ ↓ chọn · Enter / nút giữa: xác nhận · Esc: đóng</div>
    </div>
  );
}

/** Talking with an employee right in the office: they answer in a few seconds. */
export function ChatPopup({ empId, onClose, embedded }: { empId: string; onClose: () => void; embedded?: boolean }) {
  const { state, selectedProject } = useStore();
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [err, setErr] = useState("");
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const e = state?.employees.find((x) => x.id === empId);
  const msgs = (state?.messages ?? []).filter((m) => (m.from === "chairman" && m.to === empId) || (m.from === empId && m.to === "chairman")).slice(-30);
  const replying = state?.runtime[empId]?.note === "Đang trả lời Chủ tịch";
  const last = msgs[msgs.length - 1];
  const waiting = replying || (last?.from === "chairman" && Date.now() - last.ts < 60_000);

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
      await api("POST", "/api/chat", { to: empId, text: text.trim(), projectId: selectedProject ?? undefined });
      setText("");
    } catch (x) {
      setErr((x as Error).message);
    } finally {
      setSending(false);
      inputRef.current?.focus();
    }
  };
  return (
    <div className={`office-chat${embedded ? " embedded" : ""}`} role="dialog" aria-label={`Chat với ${e.name}`}>
      <div className="row between oc-head">
        <b>
          💬 {e.name} · {ROLE_LABEL[e.role]}
        </b>
        <button className="ghost" onClick={onClose} aria-label="Đóng chat">
          ✖
        </button>
      </div>
      <div className="oc-list" ref={listRef}>
        {msgs.length === 0 && <p className="muted small">Chưa có tin nhắn. Hãy chào {e.name} hoặc giao việc.</p>}
        {msgs.map((m) => (
          <div key={m.id} className={`oc-msg ${m.from === "chairman" ? "me" : "them"}`}>
            <Markdown text={m.text} />
          </div>
        ))}
        {waiting && <div className="oc-msg them typing">{e.name} đang trả lời…</div>}
      </div>
      {err && <p className="warn small">{err}</p>}
      <div className="row oc-input">
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
        <button className="primary" disabled={!text.trim() || sending} onClick={() => void send()}>
          Gửi
        </button>
      </div>
    </div>
  );
}

const ARROW = {
  up: "M5 2 9 7H1z",
  down: "M5 8 1 3h8z",
  left: "M2 5 7 1v8z",
  right: "M8 5 3 9V1z",
};

/** On-screen pad for touch screens: hold an arrow to walk, the middle button to interact. */
export function DPad({ keys, onAction, onNav }: { keys: Keys; onAction: () => void; onNav?: (dir: "up" | "down") => boolean }) {
  const hold = (dir: keyof Keys) => ({
    onPointerDown: (ev: React.PointerEvent) => {
      ev.preventDefault();
      (ev.target as Element).setPointerCapture?.(ev.pointerId);
      // While a menu is open, up / down move the selection instead of the chairman.
      if ((dir === "up" || dir === "down") && onNav?.(dir)) return;
      keys[dir] = true;
    },
    onPointerUp: () => (keys[dir] = false),
    onPointerCancel: () => (keys[dir] = false),
    onLostPointerCapture: () => (keys[dir] = false),
    onContextMenu: (ev: React.MouseEvent) => ev.preventDefault(),
  });
  const btn = (dir: keyof Keys, label: string) => (
    <button className={`dp-${dir}`} aria-label={label} {...hold(dir)}>
      <svg viewBox="0 0 10 10" aria-hidden="true">
        <path d={ARROW[dir]} />
      </svg>
    </button>
  );
  return (
    <div className="dpad" aria-label="Điều khiển Chủ tịch">
      {btn("up", "Đi lên")}
      {btn("left", "Sang trái")}
      <button
        className="dp-act"
        aria-label="Tương tác"
        onPointerDown={(ev) => {
          ev.preventDefault();
          onAction();
        }}
        onContextMenu={(ev) => ev.preventDefault()}
      >
        <svg viewBox="0 0 10 10" aria-hidden="true">
          <path d="M5 1 9 5H6v2H4V5H1zM2 8h6v1.5H2z" />
        </svg>
      </button>
      {btn("right", "Sang phải")}
      {btn("down", "Đi xuống")}
    </div>
  );
}
