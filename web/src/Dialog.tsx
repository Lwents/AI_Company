import { useEffect, useRef, useState } from "react";

/** In-app pixel dialogs, replacing the browser's native confirm()/alert(). */
export interface DialogOptions {
  title: string;
  message?: string;
  okLabel?: string;
  cancelLabel?: string;
  /** Red confirm button for destructive actions. */
  danger?: boolean;
  /** Only an OK button (alert style). */
  alert?: boolean;
  icon?: string;
}

interface Pending extends DialogOptions {
  resolve: (ok: boolean) => void;
}

let show: ((d: Pending) => void) | null = null;

export function confirmDialog(opts: DialogOptions): Promise<boolean> {
  return new Promise((resolve) => {
    if (!show) {
      resolve(false);
      return;
    }
    show({ ...opts, resolve });
  });
}

export function alertDialog(opts: Omit<DialogOptions, "alert" | "cancelLabel">): Promise<void> {
  return confirmDialog({ ...opts, alert: true }).then(() => undefined);
}

export function DialogHost() {
  const [queue, setQueue] = useState<Pending[]>([]);
  const okRef = useRef<HTMLButtonElement>(null);
  const cur = queue[0];

  useEffect(() => {
    show = (d) => setQueue((q) => [...q, d]);
    return () => {
      show = null;
    };
  }, []);

  const close = (ok: boolean) => {
    if (!cur) return;
    cur.resolve(ok);
    setQueue((q) => q.slice(1));
  };

  useEffect(() => {
    if (!cur) return;
    okRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close(false);
      } else if (e.key === "Enter") {
        e.preventDefault();
        close(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  });

  if (!cur) return null;
  const icon = cur.icon ?? (cur.alert ? "ℹ" : cur.danger ? "⚠" : "?");
  return (
    <div className="dlg-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close(false)}>
      <div className={`dlg ${cur.danger ? "danger" : ""}`} role="dialog" aria-modal="true" aria-labelledby="dlg-title">
        <div className="dlg-head">
          <span className="dlg-icon">{icon}</span>
          <b id="dlg-title">{cur.title}</b>
        </div>
        {cur.message && <p className="dlg-msg">{cur.message}</p>}
        <div className="dlg-actions">
          {!cur.alert && (
            <button className="ghost" onClick={() => close(false)}>
              {cur.cancelLabel ?? "Hủy"}
            </button>
          )}
          <button ref={okRef} className={cur.danger ? "danger" : "primary"} onClick={() => close(true)}>
            {cur.okLabel ?? "Đồng ý"}
          </button>
        </div>
      </div>
    </div>
  );
}
