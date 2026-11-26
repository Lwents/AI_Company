import { useEffect, useState } from "react";
import { api } from "@/api/http";
import { useStore } from "@/store";
import { PROVIDER_LABEL } from "@/shared/labels";
import type { ProviderId, Settings } from "@/api/types";

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
      await api("PUT", "/api/settings", draft);
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
