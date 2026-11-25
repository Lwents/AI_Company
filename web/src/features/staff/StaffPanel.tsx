import { useState } from "react";
import { api } from "@/api/http";
import { useStore } from "@/store";
import { DEPARTMENTS, ROLE_JOB, ROLES, ROLE_LABEL } from "@/shared/labels";
import type { Employee, Role } from "@/api/types";
import { EmployeeCard } from "./EmployeeCard";
import "./staff.css";

export function StaffPanel() {
  const { state, selectedEmployee, setSelectedEmployee } = useStore();
  const [newRole, setNewRole] = useState<Role>("dev");
  const [err, setErr] = useState("");
  if (!state) return null;

  const hire = async () => {
    setErr("");
    try {
      const e = await api<Employee>("POST", "/api/employees", { role: newRole });
      setSelectedEmployee(e.id);
    } catch (e) {
      setErr(String((e as Error).message));
    }
  };

  return (
    <div className="panel-body">
      <section className="card">
        <div className="row">
          <select value={newRole} onChange={(e) => setNewRole(e.target.value as Role)}>
            {ROLES.map((r) => (
              <option key={r} value={r}>
                {ROLE_LABEL[r]} — {ROLE_JOB[r]}
              </option>
            ))}
          </select>
          <button className="primary" onClick={hire}>
            Tuyển nhân viên
          </button>
        </div>
        <p className="muted small">
          Mọi vai trò đều có việc trong quy trình. Vai trò nào chưa có người thì bước đó được bỏ qua. Người mới mặc định chạy <b>Mock</b> (miễn phí); đổi sang model thật bên dưới.
        </p>
        {err && <p className="error small">{err}</p>}
      </section>
      {DEPARTMENTS.map((d) => {
        const people = state.employees.filter((e) => d.roles.includes(e.role));
        if (people.length === 0) return null;
        return (
          <div key={d.label} className="stack">
            <div className="staff-group">
              {d.label} <span className="muted small">· {people.length} người</span>
            </div>
            {people.map((e) => (
              <EmployeeCard key={e.id} e={e} open={selectedEmployee === e.id} onToggle={() => setSelectedEmployee(selectedEmployee === e.id ? null : e.id)} />
            ))}
          </div>
        );
      })}
    </div>
  );
}
