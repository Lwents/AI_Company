import { useEffect } from "react";
import { GuidePanel } from "./GuidePanel";
import "./guide.css";

export function GuideModal({ onClose }: { onClose: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="guide-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="guide-modal" role="dialog" aria-modal="true" aria-labelledby="guide-title">
        <div className="guide-head">
          <div className="row gap6">
            <span className="guide-icon" aria-hidden="true">
              <svg viewBox="0 0 12 12">
                <rect x="0" y="0" width="12" height="12" fill="#8b5a2b" />
                <rect x="1" y="1" width="10" height="4" fill="#3e2723" />
                <rect x="2" y="2" width="2" height="3" fill="#b13e53" />
                <rect x="5" y="1" width="2" height="4" fill="#ffcd75" />
                <rect x="8" y="2" width="2" height="3" fill="#41a6f6" />
                <rect x="1" y="7" width="10" height="4" fill="#3e2723" />
                <rect x="2" y="8" width="2" height="3" fill="#38b764" />
                <rect x="5" y="7" width="2" height="4" fill="#ef7d57" />
                <rect x="8" y="8" width="2" height="3" fill="#a7f070" />
              </svg>
            </span>
            <b id="guide-title">Tủ sách · Hướng dẫn sử dụng</b>
          </div>
          <button className="ghost" onClick={onClose} aria-label="Đóng">
            ✕
          </button>
        </div>
        <div className="guide-body">
          <GuidePanel />
        </div>
      </div>
    </div>
  );
}
