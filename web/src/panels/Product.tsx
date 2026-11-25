import { useEffect, useState } from "react";
import { api, useStore } from "../store";

interface FileInfo {
  path: string;
  size: number;
}

export function Product() {
  const { state, selectedProject } = useStore();
  const [files, setFiles] = useState<FileInfo[]>([]);
  const [dir, setDir] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [preview, setPreview] = useState(false);
  const [nonce, setNonce] = useState(0);
  const project = state?.projects.find((p) => p.id === selectedProject);
  // Refresh when activity happens on this project.
  const tick = state?.activity.filter((a) => a.projectId === selectedProject && a.kind === "result").length ?? 0;

  useEffect(() => {
    if (!selectedProject) return;
    api<{ dir: string; files: FileInfo[] }>("GET", `/api/projects/${selectedProject}/files`)
      .then((r) => {
        setFiles(r.files);
        setDir(r.dir);
      })
      .catch(() => setFiles([]));
  }, [selectedProject, tick]);

  useEffect(() => {
    if (!open || !selectedProject) return;
    api<{ content: string }>("GET", `/api/projects/${selectedProject}/file?path=${encodeURIComponent(open)}`)
      .then((r) => setContent(r.content))
      .catch((e) => setContent(String(e)));
  }, [open, selectedProject, tick]);

  if (!project) return <p className="muted center pad">Chọn một dự án ở tab Dự án.</p>;
  const hasIndex = files.some((f) => f.path === "index.html");

  return (
    <div className="panel-body">
      <section className="card">
        <div className="row between">
          <h3 className="clip">{project.name}</h3>
          {hasIndex && (
            <div className="row gap4">
              <button onClick={() => { setPreview(!preview); setNonce(Date.now()); }}>{preview ? "Ẩn xem thử" : "Xem thử"}</button>
              <a className="btn" href={`/preview/${project.id}/`} target="_blank" rel="noreferrer">
                Mở tab mới
              </a>
            </div>
          )}
        </div>
        <p className="muted small mono clip">{dir}</p>
        {preview && hasIndex && <iframe className="preview" title="preview" src={`/preview/${project.id}/?v=${nonce}`} sandbox="allow-scripts allow-forms allow-modals" />}
      </section>
      <section className="card">
        <h3>Tệp ({files.length})</h3>
        {files.length === 0 && <p className="muted small">Chưa có tệp nào.</p>}
        <ul className="files">
          {files.map((f) => (
            <li key={f.path} className={open === f.path ? "on" : ""} onClick={() => setOpen(open === f.path ? null : f.path)}>
              <span className="mono">{f.path}</span>
              <span className="muted small">{f.size < 1024 ? `${f.size} B` : `${(f.size / 1024).toFixed(1)} KB`}</span>
            </li>
          ))}
        </ul>
      </section>
      {open && (
        <section className="card">
          <div className="row between">
            <b className="mono">{open}</b>
            <button className="ghost" onClick={() => setOpen(null)}>
              Đóng
            </button>
          </div>
          <pre className="code">{content}</pre>
        </section>
      )}
    </div>
  );
}
