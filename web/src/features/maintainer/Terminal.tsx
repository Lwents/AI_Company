import { useEffect, useRef, useState } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { useStore } from "@/store";
import "./terminal.css";

/** Keys a phone keyboard lacks, sent as the terminal escape sequences the CLI expects. */
const KEYS: Array<[string, string]> = [
  ["Esc", "\x1b"],
  ["Tab", "\t"],
  ["⇧Tab", "\x1b[Z"],
  ["↑", "\x1b[A"],
  ["↓", "\x1b[B"],
  ["←", "\x1b[D"],
  ["→", "\x1b[C"],
  ["Ctrl+C", "\x03"],
  ["⏎", "\r"],
];

/**
 * The system maintainer = the real Claude CLI running on the server, shown in a terminal (xterm.js).
 * The session lives on the server: closing this window or the phone does not stop it.
 */
export function Terminal({
  empId,
  onClose,
  embedded,
}: {
  empId: string;
  onClose: () => void;
  embedded?: boolean;
}) {
  const { state } = useStore();
  const e = state?.employees.find((x) => x.id === empId);
  const dialogRef = useRef<HTMLDivElement>(null);
  const hostRef = useRef<HTMLDivElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const sendRef = useRef<(data: string) => void>(() => {});
  const termRef = useRef<XTerm | null>(null);
  const [status, setStatus] = useState<"connecting" | "on" | "off">(
    "connecting",
  );
  const [line, setLine] = useState("");
  const [uploading, setUploading] = useState(false);
  const [uploadErr, setUploadErr] = useState("");
  const touch =
    typeof matchMedia === "function" && matchMedia("(pointer: coarse)").matches;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const phone = host.clientWidth < 600;
    const term = new XTerm({
      cursorBlink: true,
      fontFamily:
        '"Cascadia Mono", "Cascadia Code", Consolas, "SF Mono", Menlo, ui-monospace, monospace',
      fontSize: phone ? 12 : 14,
      lineHeight: 1.15,
      scrollback: 5000,
      allowProposedApi: true,
      theme: {
        background: "#0c0c0c",
        foreground: "#e6e6e6",
        cursor: "#e6e6e6",
        selectionBackground: "#3a3d41",
      },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    termRef.current = term;
    // Prevent xterm's hidden textarea from stealing focus on mobile touch and jumping the scroll position.
    if (touch && term.textarea) {
      term.textarea.setAttribute("readonly", "readonly");
      term.textarea.setAttribute("inputmode", "none");
      term.textarea.tabIndex = -1;
    }
    const doFit = () => {
      try {
        const atBottom =
          term.buffer.active.viewportY >= term.buffer.active.baseY - 1;
        fit.fit();
        if (atBottom) term.scrollToBottom();
      } catch {
        /* not laid out yet */
      }
    };
    doFit();
    // The CLI redraws its whole screen with "erase screen" (ESC[2J) — on resize, /clear, long answers. xterm
    // keeps the old picture in the scrollback, so scrolling up showed the same transcript several times.
    // Drop the scrollback before each full redraw, like a native terminal running the CLI does.
    const eraseHook = term.parser.registerCsiHandler({ final: "J" }, (params) => {
      if (params[0] === 2) term.clear();
      return false; // the normal erase still runs
    });

    let ws: WebSocket | null = null;
    let closed = false;
    // Where the CLI session lives: the rescue service on the server (it survives deploys), else this app.
    let ptyPath = "/api/maintainer/pty";
    let retry: number | undefined;
    let fitTimer: number | undefined;
    const send = (data: string) => {
      if (ws?.readyState === WebSocket.OPEN)
        ws.send(JSON.stringify({ type: "input", data }));
    };
    sendRef.current = send;
    // Only real size changes reach the CLI (each one makes it redraw everything).
    let sent = "";
    const sendSize = () => {
      const size = `${term.cols}x${term.rows}`;
      if (size === sent || ws?.readyState !== WebSocket.OPEN) return;
      sent = size;
      ws.send(JSON.stringify({ type: "resize", cols: term.cols, rows: term.rows }));
    };
    const connect = () => {
      setStatus("connecting");
      const proto = location.protocol === "https:" ? "wss" : "ws";
      ws = new WebSocket(
        `${proto}://${location.host}${ptyPath}?cols=${term.cols}&rows=${term.rows}`,
      );
      ws.binaryType = "arraybuffer";
      ws.onopen = () => {
        setStatus("on");
        term.reset();
        sent = "";
        sendSize();
      };
      ws.onmessage = (m) => {
        if (typeof m.data === "string") term.write(m.data);
        else term.write(new Uint8Array(m.data as ArrayBuffer));
      };
      ws.onclose = () => {
        setStatus("off");
        if (!closed) retry = window.setTimeout(connect, 2000);
      };
    };
    void fetch("/api/maintainer")
      .then((r) => (r.ok ? r.json() : null))
      .then((i: { ptyPath?: string } | null) => {
        if (i?.ptyPath?.startsWith("/")) ptyPath = i.ptyPath;
      })
      .catch(() => {})
      .finally(() => {
        if (!closed) connect();
      });
    const onData = term.onData(send);
    const ro = new ResizeObserver(() => {
      window.clearTimeout(fitTimer);
      fitTimer = window.setTimeout(() => {
        doFit();
        sendSize();
      }, 120);
    });
    ro.observe(host);
    // Keep the mobile overlay locked to the visual viewport so opening the keyboard doesn't push it up.
    const vv = !embedded && touch ? window.visualViewport : null;
    const syncViewport = () => {
      const box = dialogRef.current;
      if (!vv || !box) return;
      box.style.height = `${Math.round(vv.height)}px`;
      box.style.top = `${Math.round(vv.offsetTop)}px`;
    };
    if (vv) {
      syncViewport();
      vv.addEventListener("resize", syncViewport);
      vv.addEventListener("scroll", syncViewport);
    }
    if (!touch) term.focus();
    return () => {
      closed = true;
      window.clearTimeout(retry);
      window.clearTimeout(fitTimer);
      if (vv) {
        vv.removeEventListener("resize", syncViewport);
        vv.removeEventListener("scroll", syncViewport);
      }
      ro.disconnect();
      onData.dispose();
      eraseHook.dispose();
      ws?.close();
      term.dispose();
      termRef.current = null;
    };
  }, [touch, embedded]);

  if (!e) return null;
  const uploadImage = async (file: File) => {
    setUploadErr("");
    setUploading(true);
    try {
      const res = await fetch("/api/maintainer/upload", {
        method: "POST",
        headers: { "Content-Type": file.type || "application/octet-stream" },
        body: file,
      });
      if (!res.ok) {
        const msg = await res.text().catch(() => "");
        setUploadErr(msg || "Không tải ảnh lên được");
        return;
      }
      const data = (await res.json()) as { path?: string };
      if (!data.path) return;
      if (touch) {
        setLine((prev) => (prev ? `${prev.trimEnd()} ${data.path} ` : `${data.path} `));
        inputRef.current?.focus();
      } else {
        sendRef.current(`${data.path} `);
        termRef.current?.focus();
      }
    } catch {
      setUploadErr("Lỗi mạng khi gửi ảnh");
    } finally {
      setUploading(false);
    }
  };
  const sendLine = () => {
    if (!line.trim()) return;
    sendRef.current(line);
    window.setTimeout(() => sendRef.current("\r"), 30);
    setLine("");
  };
  return (
    <div
      ref={dialogRef}
      className={`office-chat term${embedded ? " embedded" : ""}`}
      role="dialog"
      aria-label={`Claude CLI của ${e.name}`}
      onPaste={(ev) => {
        const items = ev.clipboardData?.items;
        if (!items) return;
        for (const item of items) {
          if (item.type.startsWith("image/")) {
            const f = item.getAsFile();
            if (f) {
              ev.preventDefault();
              void uploadImage(f);
              break;
            }
          }
        }
      }}
    >
      <input
        ref={fileRef}
        type="file"
        accept="image/png,image/jpeg,image/webp,image/gif"
        hidden
        onChange={(ev) => {
          const f = ev.target.files?.[0];
          ev.target.value = "";
          if (f) void uploadImage(f);
        }}
      />
      <div className="term-bar">
        <span className={`term-led ${status}`} />
        <b>{e.name}</b>
        <span className="dim">· Bảo trì hệ thống · Claude CLI</span>
        <span className="grow" />
        <span className="dim small">
          {uploadErr
            ? uploadErr
            : uploading
              ? "đang gửi ảnh…"
              : status === "on"
                ? "đã kết nối"
                : status === "connecting"
                  ? "đang nối…"
                  : "mất kết nối, đang nối lại…"}
        </span>
        <button
          className="term-close"
          onClick={onClose}
          aria-label="Đóng (phiên CLI vẫn chạy trên máy chủ)"
          title="Đóng — phiên CLI vẫn chạy trên máy chủ"
        >
          ✕
        </button>
      </div>
      <div
        className="term-host"
        ref={hostRef}
        onClick={() => {
          if (!touch) termRef.current?.focus();
        }}
      />
      <div className="term-keys">
        {!touch && (
          <button
            disabled={uploading}
            onPointerDown={(ev) => ev.preventDefault()}
            onClick={() => fileRef.current?.click()}
            title="Chọn ảnh gửi vào đoạn chat (hoặc Ctrl+V để dán)"
          >
            {uploading ? "Đang tải…" : "Gửi ảnh"}
          </button>
        )}
        {KEYS.map(([label, seq]) => (
          <button
            key={label}
            onPointerDown={(ev) => ev.preventDefault()}
            onClick={() => {
              sendRef.current(seq);
              if (!touch) termRef.current?.focus();
            }}
          >
            {label}
          </button>
        ))}
      </div>
      {touch && (
        <div className="term-line">
          <button
            type="button"
            className="t-img"
            disabled={uploading}
            onClick={() => fileRef.current?.click()}
            aria-label="Gửi ảnh"
            title="Chọn ảnh gửi vào đoạn chat"
          >
            {uploading ? "…" : "Ảnh"}
          </button>
          <input
            ref={inputRef}
            value={line}
            placeholder="Gõ tin nhắn (có dấu) rồi Gửi"
            onChange={(ev) => setLine(ev.target.value)}
            onKeyDown={(ev) => {
              if (ev.key === "Enter" && !ev.nativeEvent.isComposing) {
                ev.preventDefault();
                sendLine();
              }
            }}
          />
          <button onClick={sendLine}>Gửi</button>
        </div>
      )}
    </div>
  );
}
