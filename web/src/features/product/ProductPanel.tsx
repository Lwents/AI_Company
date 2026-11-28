import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { api } from "@/api/http";
import { useStore } from "@/store";
import "./product.css";
import { previewUrl } from "@/shared/runtime";

interface FileInfo {
  path: string;
  size: number;
}

interface TreeNode {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  fileCount: number;
  children: TreeNode[];
}

interface FileResponse {
  path: string;
  kind?: "text" | "image" | "binary";
  size?: number;
  truncated?: boolean;
  content: string;
}

function fmtSize(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MB`;
}

function isImagePath(p: string) {
  return /\.(png|jpe?g|gif|webp|ico|bmp|svg)$/i.test(p);
}

function buildTree(files: FileInfo[]): TreeNode[] {
  const root: TreeNode = { name: "", path: "", isDir: true, size: 0, fileCount: 0, children: [] };
  const dirMap = new Map<string, TreeNode>([["", root]]);

  for (const f of files) {
    const parts = f.path.replace(/\\/g, "/").split("/").filter(Boolean);
    let curPath = "";
    let parent = root;
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      const nextPath = curPath ? `${curPath}/${part}` : part;
      const isLast = i === parts.length - 1;
      if (isLast) {
        parent.children.push({
          name: part,
          path: f.path,
          isDir: false,
          size: f.size,
          fileCount: 1,
          children: [],
        });
      } else {
        let dir = dirMap.get(nextPath);
        if (!dir) {
          dir = { name: part, path: nextPath, isDir: true, size: 0, fileCount: 0, children: [] };
          dirMap.set(nextPath, dir);
          parent.children.push(dir);
        }
        parent = dir;
      }
      curPath = nextPath;
    }
  }

  function finalize(node: TreeNode): { count: number; size: number } {
    if (!node.isDir) return { count: 1, size: node.size };
    let count = 0;
    let size = 0;
    for (const c of node.children) {
      const r = finalize(c);
      count += r.count;
      size += r.size;
    }
    node.fileCount = count;
    node.size = size;
    // Folders first (non-dot folders before hidden dot-folders like .playwright-mcp), then files alphabetically.
    node.children.sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      const aDot = a.name.startsWith(".");
      const bDot = b.name.startsWith(".");
      if (aDot !== bDot) return aDot ? 1 : -1;
      return a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
    });
    return { count, size };
  }
  finalize(root);
  return root.children;
}

function collectDirPaths(nodes: TreeNode[], out: string[] = []): string[] {
  for (const n of nodes) {
    if (n.isDir) {
      out.push(n.path);
      collectDirPaths(n.children, out);
    }
  }
  return out;
}

export function ProductPanel() {
  const { state, selectedProject } = useStore();
  const [files, setFiles] = useState<FileInfo[]>([]);
  const [dir, setDir] = useState("");
  const [serverHasPreview, setServerHasPreview] = useState(false);
  const [filter, setFilter] = useState("");
  const [hideHidden, setHideHidden] = useState(true);
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const [open, setOpen] = useState<string | null>(null);
  const [fileData, setFileData] = useState<FileResponse | null>(null);
  const [loadingFile, setLoadingFile] = useState(false);
  const [reloadNonce, setReloadNonce] = useState(0);
  const [preview, setPreview] = useState(false);
  const [nonce, setNonce] = useState(0);
  const lastListFetch = useRef(0);
  const project = state?.projects.find((p) => p.id === selectedProject);
  // Bucket activity updates (every 5 results) so we don't rescan hundreds of files every 2 seconds.
  const tick = Math.floor((state?.activity.filter((a) => a.projectId === selectedProject && a.kind === "result").length ?? 0) / 5);

  useEffect(() => {
    setFiles([]);
    setDir("");
    setServerHasPreview(false);
    setOpen(null);
    setFileData(null);
    setFilter("");
    setExpanded(new Set());
    setNonce(Date.now());
    lastListFetch.current = 0;
  }, [selectedProject]);

  useEffect(() => {
    if (!selectedProject) return;
    const now = Date.now();
    if (lastListFetch.current > 0 && now - lastListFetch.current < 6000) return;
    lastListFetch.current = now;
    let cancelled = false;
    api<{ dir: string; files: FileInfo[]; hasPreview?: boolean }>("GET", `/api/projects/${selectedProject}/files`)
      .then((r) => {
        if (cancelled) return;
        setFiles(r.files);
        setDir(r.dir);
        setServerHasPreview(Boolean(r.hasPreview));
      })
      .catch(() => {
        if (!cancelled) {
          setFiles([]);
          setServerHasPreview(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [selectedProject, tick, reloadNonce]);

  useEffect(() => {
    if (!open || !selectedProject) {
      setFileData(null);
      return;
    }
    let cancelled = false;
    setLoadingFile(true);
    api<FileResponse>("GET", `/api/projects/${selectedProject}/file?path=${encodeURIComponent(open)}`)
      .then((r) => {
        if (!cancelled) setFileData(r);
      })
      .catch((e) => {
        if (!cancelled) setFileData({ path: open, kind: "text", content: String(e) });
      })
      .finally(() => {
        if (!cancelled) setLoadingFile(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, selectedProject, reloadNonce]);

  const hiddenCount = useMemo(
    () => files.filter((f) => f.path.replace(/\\/g, "/").split("/").some((seg) => seg.startsWith(".") && seg !== ".gitignore")).length,
    [files]
  );

  const shownFiles = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return files.filter((f) => {
      if (hideHidden && !q) {
        const segs = f.path.replace(/\\/g, "/").split("/");
        if (segs.some((seg) => seg.startsWith(".") && seg !== ".gitignore")) return false;
      }
      if (!q) return true;
      return f.path.toLowerCase().includes(q);
    });
  }, [files, filter, hideHidden]);

  const tree = useMemo(() => buildTree(shownFiles), [shownFiles]);
  const searching = filter.trim().length > 0;

  function toggleDir(path: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  function renderNodes(nodes: TreeNode[], depth: number): ReactNode[] {
    const rows: ReactNode[] = [];
    for (const n of nodes) {
      const pad = { paddingLeft: `${6 + depth * 14}px` };
      if (n.isDir) {
        const isOpen = searching || expanded.has(n.path);
        rows.push(
          <li key={`d:${n.path}`} className="tree-dir" style={pad} onClick={() => toggleDir(n.path)}>
            <span className="mono clip">
              <span className="tree-caret">{isOpen ? "▾" : "▸"}</span> 📁 {n.name}/
            </span>
            <span className="muted small nowrap">{n.fileCount} tệp</span>
          </li>
        );
        if (isOpen) {
          rows.push(...renderNodes(n.children, depth + 1));
        }
      } else {
        const icon = isImagePath(n.name) ? "🖼" : "📄";
        rows.push(
          <li
            key={`f:${n.path}`}
            className={open === n.path ? "on" : ""}
            style={pad}
            title={n.path}
            onClick={() => setOpen(open === n.path ? null : n.path)}
          >
            <span className="mono clip">
              <span className="tree-icon">{icon}</span> {n.name}
            </span>
            <span className="muted small nowrap">{fmtSize(n.size)}</span>
          </li>
        );
      }
    }
    return rows;
  }

  if (!project) return <p className="muted center pad">Chọn một dự án ở tab Dự án.</p>;
  const hasIndex =
    serverHasPreview ||
    files.some((f) =>
      [
        "index.html",
        "dist/index.html",
        "build/index.html",
        "frontend/index.html",
        "frontend/dist/index.html",
        "web/index.html",
        "web/dist/index.html",
        "app/index.html",
        "app/dist/index.html",
        "public/index.html",
      ].includes(f.path),
    );
  const rawUrl = open ? `/api/projects/${selectedProject}/file/raw?path=${encodeURIComponent(open)}&v=${reloadNonce}` : "";

  return (
    <div className="panel-body">
      <section className="card">
        <div className="row between">
          <h3 className="clip">{project.name}</h3>
          {hasIndex && (
            <div className="row gap4">
              <button onClick={() => { setPreview(!preview); setNonce(Date.now()); }}>{preview ? "Ẩn xem thử" : "Xem thử"}</button>
              <a className="btn" href={previewUrl(project.id)} target="_blank" rel="noreferrer">
                Mở tab mới
              </a>
            </div>
          )}
        </div>
        <p className="muted small mono clip">{dir}</p>
        {preview && hasIndex && <iframe className="preview" title="preview" src={previewUrl(project.id, `?v=${nonce}`)} sandbox="allow-scripts allow-forms allow-modals" />}
      </section>
      {open && (
        <section className="card">
          <div className="row between gap4">
            <b className="mono clip" title={open}>
              {open}
            </b>
            <div className="row gap4">
              <button className="ghost" onClick={() => setReloadNonce((n) => n + 1)}>
                Tải lại
              </button>
              <button className="ghost" onClick={() => setOpen(null)}>
                Đóng
              </button>
            </div>
          </div>
          {loadingFile && <p className="muted small">Đang tải tệp...</p>}
          {!loadingFile && fileData?.kind === "image" && (
            <div className="file-preview-wrap">
              <img className="file-preview-img" src={rawUrl} alt={open} loading="lazy" />
              <p className="muted small">
                Ảnh ({fmtSize(fileData.size ?? 0)}) ·{" "}
                <a href={rawUrl} target="_blank" rel="noreferrer">
                  Mở ảnh gốc
                </a>
              </p>
            </div>
          )}
          {!loadingFile && fileData?.kind === "binary" && (
            <p className="muted small">
              Tệp nhị phân ({fmtSize(fileData.size ?? 0)}) — không hiển thị dạng văn bản để tránh đơ trình duyệt.{" "}
              <a href={rawUrl} target="_blank" rel="noreferrer">
                Tải tệp gốc
              </a>
            </p>
          )}
          {!loadingFile && (!fileData?.kind || fileData.kind === "text") && fileData && (
            <>
              {fileData.truncated && (
                <p className="muted small">
                  Đang hiển thị 50.000 ký tự đầu ({fmtSize(fileData.size ?? 0)}) để giữ mượt giao diện ·{" "}
                  <a href={rawUrl} target="_blank" rel="noreferrer">
                    Mở tệp đầy đủ
                  </a>
                </p>
              )}
              <pre className="code file-code">{fileData.content}</pre>
            </>
          )}
        </section>
      )}
      <section className="card">
        <div className="row between gap4">
          <h3>
            Cây thư mục ({shownFiles.length}
            {shownFiles.length !== files.length ? `/${files.length}` : ""})
          </h3>
          <div className="row gap4">
            <button className="ghost" onClick={() => setExpanded(new Set(collectDirPaths(tree)))}>
              Mở hết
            </button>
            <button className="ghost" onClick={() => setExpanded(new Set())}>
              Thu gọn
            </button>
            <button
              className="ghost"
              onClick={() => {
                lastListFetch.current = 0;
                setReloadNonce((n) => n + 1);
              }}
            >
              Làm mới
            </button>
          </div>
        </div>
        {files.length > 10 && (
          <input
            className="file-filter"
            type="text"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Lọc tệp theo tên (vd: auto_runner, server, .png)..."
          />
        )}
        {hiddenCount > 0 && (
          <label className="tree-toggle muted small">
            <input type="checkbox" checked={hideHidden} onChange={(e) => setHideHidden(e.target.checked)} />
            Ẩn tệp/thư mục tạm (`.playwright-mcp`, `.server.*` — {hiddenCount} tệp)
          </label>
        )}
        {shownFiles.length === 0 && <p className="muted small">{files.length === 0 ? "Chưa có tệp nào." : "Không khớp tệp nào."}</p>}
        <ul className="files">{renderNodes(tree, 0)}</ul>
      </section>
    </div>
  );
}
