import { useStore } from "../store";
import { ROLE_LABEL, num, usd } from "../labels";

export function Costs() {
  const { state } = useStore();
  if (!state) return null;
  const rows = state.employees.map((e) => ({ e, u: state.usage[e.id] ?? { calls: 0, inputTokens: 0, outputTokens: 0, costUsd: 0 } }));
  const total = rows.reduce((s, r) => s + r.u.costUsd, 0);
  const max = Math.max(0.0000001, ...rows.map((r) => r.u.costUsd));

  return (
    <div className="panel-body">
      <section className="card">
        <div className="big">{usd(total)}</div>
        <p className="muted small">Tổng chi phí token (ước tính theo giá bạn đặt cho từng nhân viên; Mock tính $0).</p>
      </section>
      <section className="card">
        <h3>Theo nhân viên</h3>
        <table>
          <thead>
            <tr>
              <th>Người</th>
              <th>Lượt</th>
              <th>Token vào/ra</th>
              <th>Chi phí</th>
            </tr>
          </thead>
          <tbody>
            {rows.map(({ e, u }) => (
              <tr key={e.id}>
                <td>
                  <span className="dot" style={{ background: e.color }} /> {e.name} <span className="muted">{ROLE_LABEL[e.role]}</span>
                  <div className="minibar">
                    <div style={{ width: `${(u.costUsd / max) * 100}%`, background: e.color }} />
                  </div>
                </td>
                <td>{num(u.calls)}</td>
                <td className="small">
                  {num(u.inputTokens)} / {num(u.outputTokens)}
                </td>
                <td>{usd(u.costUsd)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
      <section className="card">
        <h3>Theo dự án</h3>
        {state.projects.length === 0 && <p className="muted small">Chưa có dự án.</p>}
        {state.projects.map((p) => {
          const pct = p.budgetUsd > 0 ? Math.min(100, (p.spentUsd / p.budgetUsd) * 100) : 0;
          return (
            <div key={p.id} className="stack-sm">
              <div className="row between small">
                <span className="clip">{p.name}</span>
                <span>
                  {usd(p.spentUsd)} / {usd(p.budgetUsd)}
                </span>
              </div>
              <div className="bar">
                <div style={{ width: `${pct}%` }} className={pct > 90 ? "hot" : ""} />
              </div>
            </div>
          );
        })}
      </section>
    </div>
  );
}
