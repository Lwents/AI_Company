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
  const [srcMode, setSrcMode] = useState<"new" | "zip" | "git" | "local">("new");
  const [zip, setZip] = useState<File | null>(null);
  const [gitUrl, setGitUrl] = useState("");
  const [localPath, setLocalPath] = useState("");
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  if (!state) return null;
  const projects = [...state.projects].sort((a, b) => b.createdAt - a.createdAt);

  const submit = async () => {
    setErr("");
    setBusy(true);
    try {
      let uploadId = "";
      if (srcMode === "zip") {
        if (!zip) throw new Error("Hãy chọn tệp .zip mã nguồn");
        const res = await fetch("/api/uploads", { method: "POST", headers: { "Content-Type": "application/zip" }, body: zip });
        if (!res.ok) throw new Error(await res.text());
        uploadId = ((await res.json()) as { id: string }).id;
      }
      if (srcMode === "git" && !gitUrl.trim()) throw new Error("Hãy nhập link Git");
      if (srcMode === "local" && !localPath.trim()) throw new Error("Hãy nhập đường dẫn thư mục");
      const p = await api<Project>("POST", "/api/projects", {
        request,
        name,
        budgetUsd: budget ? Number(budget) : undefined,
        uploadId,
        uploadName: zip?.name ?? "",
        gitUrl: srcMode === "git" ? gitUrl : "",
        localPath: srcMode === "local" ? localPath : "",
      });
      setZip(null);
      setGitUrl("");
      setLocalPath("");
      setSrcMode("new");
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
          placeholder="Ví dụ: Làm cho tôi trang web quản lý chi tiêu cá nhân, có biểu đồ theo tháng… (hoặc với mã nguồn của khách: Thêm đăng nhập, sửa lỗi giỏ hàng…)"
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
        <div className="src-mode">
          <span className="small">Mã nguồn</span>
          <div className="seg" role="radiogroup">
            {(
              [
                ["new", "Làm mới"],
                ["zip", "Tải .zip"],
                ["git", "Link Git"],
                ["local", "Thư mục máy"],
              ] as const
            ).map(([v, l]) => (
              <button key={v} type="button" role="radio" aria-checked={srcMode === v} className={srcMode === v ? "primary" : "ghost"} onClick={() => setSrcMode(v)}>
                {l}
              </button>
            ))}
          </div>
        </div>
        {srcMode === "zip" && (
          <label className="file-pick">
            <input type="file" accept=".zip,application/zip" onChange={(e) => setZip(e.target.files?.[0] ?? null)} />
            <span className="btn">📁 Chọn tệp .zip</span>
            <span className={`clip small ${zip ? "" : "muted"}`}>{zip ? `${zip.name} · ${(zip.size / 1024 / 1024).toFixed(1)} MB` : "Chưa chọn tệp"}</span>
          </label>
        )}
        {srcMode === "git" && <input placeholder="https://github.com/ten/du-an.git" value={gitUrl} onChange={(e) => setGitUrl(e.target.value)} />}
        {srcMode === "local" && <input placeholder="Đường dẫn thư mục, vd. D:\du-an\cua-khach" value={localPath} onChange={(e) => setLocalPath(e.target.value)} />}
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
