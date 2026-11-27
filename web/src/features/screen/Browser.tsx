import { useEffect, useRef, useState } from "react";
import { api } from "@/api/http";
import type { ScreenEvent } from "@/api/types";
import { previewUrl } from "@/shared/runtime";

type Web = Extract<ScreenEvent, { app: "browser" }>;

const SPECIAL: Record<string, string> = {
  Enter: "Enter",
  Backspace: "Backspace",
  Tab: "Tab",
  Escape: "Escape",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
  Delete: "Delete",
};

/** A project page the employee opened (…/preview/<project>/<path>), as a link the chairman's own browser can load. */
function directOf(url: string | null | undefined): string | null {
  if (!url) return null;
  try {
    const u = new URL(url);
    const m = u.pathname.match(/^\/preview\/([^/]+)\/(.*)$/);
    return m ? previewUrl(m[1], m[2] + u.search + u.hash) : null;
  } catch {
    return null;
  }
}

/** Chrome look-alike showing the employee's real page. The chairman can use it: click, scroll, type, open an address. */
export function Browser({ webs, empId, empName, projectId }: { webs: Web[]; empId: string; empName: string; projectId: string | null }) {
  const last = webs[webs.length - 1];
  const shot = [...webs].reverse().find((w) => w.image);
  const url = [...webs].reverse().find((w) => w.url)?.url ?? "about:blank";
  const title = [...webs].reverse().find((w) => w.title)?.title ?? "Tab mới";
  const [addr, setAddr] = useState<string | null>(null);
  // "direct": the chairman uses the same page in their own browser (instant); "remote": the employee's machine.
  const followed = directOf(url) ?? (projectId ? previewUrl(projectId) : null);
  const [mode, setMode] = useState<"direct" | "remote">(followed ? "direct" : "remote");
  const [own, setOwn] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const frameRef = useRef<HTMLIFrameElement>(null);
  const frameSrc = own ?? followed;
  const direct = mode === "direct" && !!frameSrc;
  // What kind of page this is, so the chairman knows whether buttons are supposed to work.
  const path = frameSrc ? decodeURIComponent(frameSrc.replace(/^.*?\/preview\/[^/]+\//, "")) : "";
  const kind = /^tests?\//.test(path) ? "test" : /^design\//.test(path) ? "design" : /^docs\//.test(path) ? "docs" : "product";
  const [missing, setMissing] = useState(false);
  useEffect(() => {
    setMissing(false);
    if (!direct || !frameSrc) return;
    let alive = true;
    fetch(frameSrc, { method: "HEAD" })
      .then((r) => alive && setMissing(r.status === 404))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [direct, frameSrc, reload]);
  const home = projectId ? previewUrl(projectId) : null;
  const [sentAt, setSentAt] = useState(0);
  const [err, setErr] = useState("");
  const [ripple, setRipple] = useState<{ x: number; y: number; k: number } | null>(null);
  const [typing, setTyping] = useState("");
  const imgRef = useRef<HTMLImageElement>(null);
  const wheel = useRef({ dy: 0, timer: 0 });
  const keys = useRef({ text: "", timer: 0 });
  const drag = useRef<{ x: number; y: number; moved: boolean } | null>(null);
  const waiting = sentAt > 0 && (last?.ts ?? 0) < sentAt && Date.now() - sentAt < 20_000;
  // Re-render once a second while waiting, so the hint goes away on its own.
  const [, tick] = useState(0);
  useEffect(() => {
    if (!waiting) return;
    const t = setTimeout(() => tick((n) => n + 1), 1000);
    return () => clearTimeout(t);
  });

  const send = async (cmd: Record<string, unknown>) => {
    setErr("");
    try {
      await api("POST", `/api/screen/${empId}/input`, cmd);
      setSentAt(Date.now());
    } catch (e) {
      setErr((e as Error).message);
    }
  };
  /** Screen point → pixels of the page in the screenshot. */
  const toPage = (clientX: number, clientY: number) => {
    const img = imgRef.current!;
    const r = img.getBoundingClientRect();
    const k = img.naturalWidth / r.width;
    return { x: (clientX - r.left) * k, y: (clientY - r.top) * k, sx: clientX - r.left, sy: clientY - r.top };
  };
  const click = (clientX: number, clientY: number) => {
    if (!imgRef.current) return;
    const pt = toPage(clientX, clientY);
    setRipple({ x: pt.sx, y: pt.sy, k: Date.now() });
    void send({ kind: "click", x: pt.x, y: pt.y, full: !!shot?.full });
  };
  const flushKeys = () => {
    const t = keys.current.text;
    keys.current.text = "";
    if (t) void send({ kind: "type", text: t });
  };

  return (
    <div className="chrome">
      <div className="chrome-tabs">
        <span className="dots">
          <i /> <i /> <i />
        </span>
        <span className="chrome-tab">{direct ? path.split("/").pop() || "Trang sản phẩm" : title}</span>
      </div>
      <div className="chrome-bar">
        <button
          className="nav-b"
          title="Quay lại"
          aria-label="Quay lại"
          onClick={() => {
            if (!direct) return void send({ kind: "back" });
            try {
              frameRef.current?.contentWindow?.history.back();
            } catch {
              /* page on another origin */
            }
          }}
        >
          ←
        </button>
        <button className="nav-b" title="Tải lại" aria-label="Tải lại" onClick={() => (direct ? setReload((n) => n + 1) : url.startsWith("http") && void send({ kind: "goto", url }))}>
          ⟳
        </button>
        <form
          className="addr"
          onSubmit={(e) => {
            e.preventDefault();
            const u = (addr ?? url).trim();
            setAddr(null);
            (document.activeElement as HTMLElement | null)?.blur();
            if (!u) return;
            const full = /^https?:\/\//.test(u) ? u : `https://${u}`;
            if (!direct) return void send({ kind: "goto", url: full });
            const d = directOf(full);
            if (d) setOwn(d);
            else setErr("Chế độ Tự dùng chỉ mở trang của dự án. Trang ngoài: chuyển sang 👀 Máy nhân viên.");
          }}
        >
          <span aria-hidden="true">🔒</span>
          <input value={addr ?? (direct ? frameSrc! : url)} onChange={(e) => setAddr(e.target.value)} onBlur={() => setAddr(null)} aria-label="Địa chỉ trang" spellCheck={false} />
        </form>
        <div className="mode-seg" role="group" aria-label="Chế độ trình duyệt">
          <button className={direct ? "on" : ""} disabled={!frameSrc} onClick={() => setMode("direct")} title="Tự dùng trang của dự án ngay trên máy bạn: bấm, cuộn, gõ tức thì">
            🖐 Tự dùng
          </button>
          <button className={direct ? "" : "on"} onClick={() => setMode("remote")} title={`Xem và điều khiển trình duyệt trên máy ${empName}`}>
            👀 Máy {empName}
          </button>
        </div>
      </div>
      <div className="chrome-page">
        {direct ? (
          <iframe key={`${frameSrc}|${reload}`} ref={frameRef} className="page-frame" src={frameSrc!} title={`Trang dự án (bạn tự dùng)`} />
        ) : shot?.image ? (
          <div
            className="page-live"
            tabIndex={0}
            title={`Bấm, cuộn, gõ phím để dùng trình duyệt của ${empName}`}
            onWheel={(e) => {
              // Gather a burst of wheel ticks into one scroll.
              wheel.current.dy += e.deltaY;
              if (wheel.current.timer) return;
              wheel.current.timer = window.setTimeout(() => {
                const dy = Math.round(wheel.current.dy);
                wheel.current = { dy: 0, timer: 0 };
                if (dy) void send({ kind: "scroll", dy });
              }, 350);
            }}
            onPointerDown={(e) => {
              drag.current = { x: e.clientX, y: e.clientY, moved: false };
            }}
            onPointerMove={(e) => {
              const d = drag.current;
              if (d && Math.hypot(e.clientX - d.x, e.clientY - d.y) > 10) d.moved = true;
            }}
            onPointerUp={(e) => {
              const d = drag.current;
              drag.current = null;
              if (!d) return;
              if (d.moved) {
                // A swipe scrolls the page the other way, like a finger on a phone.
                const img = imgRef.current;
                const k = img ? img.naturalWidth / img.getBoundingClientRect().width : 1;
                void send({ kind: "scroll", dy: Math.round((d.y - e.clientY) * k) });
              } else click(e.clientX, e.clientY);
            }}
            onPointerCancel={() => (drag.current = null)}
            onKeyDown={(e) => {
              if (e.ctrlKey || e.metaKey || e.altKey) return;
              if (SPECIAL[e.key]) {
                e.preventDefault();
                flushKeys();
                void send({ kind: "key", key: SPECIAL[e.key] });
              } else if (e.key.length === 1) {
                e.preventDefault();
                keys.current.text += e.key;
                clearTimeout(keys.current.timer);
                keys.current.timer = window.setTimeout(flushKeys, 500);
              }
            }}
          >
            <img ref={imgRef} src={shot.image} alt={`Trang ${title} trên máy ${empName}`} draggable={false} />
            {ripple && <span key={ripple.k} className="ripple" style={{ left: ripple.x, top: ripple.y }} />}
          </div>
        ) : (
          <div className="desk-idle">{webs.length ? "Đang mở trang…" : `${empName} chưa mở trình duyệt.`}</div>
        )}
        <div className="chrome-actions" aria-label="Thao tác vừa làm" hidden={direct}>
          {webs.slice(-5).map((w) => (
            <span key={w.ts} className={`${w === last ? "on" : ""}${w.action.startsWith("👑") ? " me" : ""}`}>
              {w.action.startsWith("👑") ? "" : "🖱 "}
              {w.action.replace(/^\[browser\]\s*/, "")}
            </span>
          ))}
        </div>
      </div>
      <div className="chrome-ctl">
        {direct ? (
          <div className="chrome-hint">
            {missing ? (
              <span className="err">⚠ Trang này chưa có (404): đội chưa làm tới phần này của sản phẩm.</span>
            ) : kind === "test" ? (
              <span className="warn-t">⚠ Đây là trang thử do Tester dựng để đo / kiểm tra (không phải sản phẩm), các nút có thể không hoạt động.</span>
            ) : kind === "design" ? (
              <span className="warn-t">ℹ Đây là bản mẫu thiết kế (tĩnh), chưa phải sản phẩm thật.</span>
            ) : null}{" "}
            {home && frameSrc !== home && (
              <button className="link-b" onClick={() => setOwn(home)}>
                🏠 Mở trang sản phẩm
              </button>
            )}{" "}
            {err ? (
              <span className="err">⚠ {err}</span>
            ) : (
              <span>
                🖐 Bạn đang tự dùng trang này (tức thì, tách biệt với máy {empName}).{" "}
                {own ? (
                  <button className="link-b" onClick={() => setOwn(null)}>
                    ↩ Mở theo trang {empName} đang xem
                  </button>
                ) : (
                  `Trang tự đổi theo trang ${empName} đang xem.`
                )}
              </span>
            )}
          </div>
        ) : (
        <>
        <form
          className="chrome-type"
          onSubmit={(e) => {
            e.preventDefault();
            if (!typing.trim()) return;
            void send({ kind: "type", text: typing });
            setTyping("");
          }}
        >
          <input value={typing} onChange={(e) => setTyping(e.target.value)} placeholder="⌨ Gõ chữ vào ô đang chọn trên trang…" aria-label="Gõ chữ vào trang" />
          <button type="submit">Gõ</button>
          <button type="button" onClick={() => void send({ kind: "key", key: "Enter" })}>
            ↵ Enter
          </button>
        </form>
        <div className="chrome-hint">
          {err ? (
            <span className="err">⚠ {err}</span>
          ) : waiting ? (
            <span>⏳ Đang gửi… lệnh chạy ngay khi {empName} rảnh tay (lúc đang suy nghĩ thì chạy liền).</span>
          ) : (
            <span>Bấm vào trang để bấm · lăn chuột / vuốt để cuộn · bấm chọn trang rồi gõ phím.</span>
          )}
        </div>
        </>
        )}
      </div>
    </div>
  );
}
