import { useState } from "react";
import { api } from "@/api/http";
import { useStore } from "@/store";
import type { Project } from "@/api/types";
import { ProjectCard } from "./ProjectCard";
import "./projects.css";

export function ProjectsPanel() {
  const { state, selectedProject, setSelectedProject } = useStore();
  const [request, setRequest] = useState("");
  const [name, setName] = useState("");
  const [budget, setBudget] = useState("");
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  if (!state) return null;
  const projects = [...state.projects].sort((a, b) => b.createdAt - a.createdAt);

  const submit = async () => {
    setErr("");
    setBusy(true);
    try {
      const p = await api<Project>("POST", "/api/projects", {
        request,
        name,
        budgetUsd: budget ? Number(budget) : undefined,
      });
      setRequest("");
      setName("");
      setBudget("");
      setSelectedProject(p.id);
    } catch (e) {
      setErr(String((e as Error).message));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="panel-body">
      <section className="card">
        <h3>Giao dự án mới cho CEO</h3>
        <textarea
          rows={4}
          placeholder="Ví dụ: Làm cho tôi trang web quản lý chi tiêu cá nhân, có biểu đồ theo tháng…"
          value={request}
          onChange={(e) => setRequest(e.target.value)}
        />
        <div className="row">
          <input placeholder="Tên dự án (tùy chọn)" value={name} onChange={(e) => setName(e.target.value)} />
          <input
            className="w-90"
            type="number"
            min={0}
            step={0.5}
            placeholder={`Ngân sách $${state.settings.defaultBudgetUsd}`}
            value={budget}
            onChange={(e) => setBudget(e.target.value)}
          />
        </div>
        {err && <p className="error">{err}</p>}
        <button className="primary" disabled={busy || !request.trim()} onClick={submit}>
          Giao cho CEO
        </button>
      </section>

      {projects.length === 0 && <p className="muted center">Chưa có dự án nào.</p>}
      {projects.map((p) => (
        <ProjectCard key={p.id} p={p} selected={p.id === selectedProject} onSelect={() => setSelectedProject(p.id)} />
      ))}
    </div>
  );
}
