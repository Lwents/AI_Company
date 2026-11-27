import { memo, useEffect, useMemo, useRef, useState } from "react";
import { api } from "@/api/http";
import type { ScreenEvent } from "@/api/types";
import { highlight, langOf, type Tok } from "./highlight";

type Code = Extract<ScreenEvent, { app: "code" }>;
type Term = Extract<ScreenEvent, { app: "terminal" }>;

/** Counts from 0 to `n` over `ms` (restarts when `key` changes). */
export function useTyping(n: number, ms: number, key: unknown) {
  const [v, setV] = useState(0);
  useEffect(() => {
    if (n <= 0) {
      setV(0);
      return;
    }
    const t0 = performance.now();
    setV(0);
    const timer = setInterval(() => {
      const k = Math.min(1, (performance.now() - t0) / ms);
      setV(Math.round(n * k));
      if (k >= 1) clearInterval(timer);
    }, 40);
    return () => clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return v;
}

function common(a: string, b: string): [number, number] {
  let p = 0;
  const max = Math.min(a.length, b.length);
  while (p < max && a.charCodeAt(p) === b.charCodeAt(p)) p++;
  let s = 0;
  while (s < max - p && a.charCodeAt(a.length - 1 - s) === b.charCodeAt(b.length - 1 - s)) s++;
  return [p, s];
}

/* ---------- icons ---------- */

const FILE_ICON: Record<string, [string, string]> = {
  js: ["JS", "#f1dd3f"],
  mjs: ["JS", "#f1dd3f"],
  cjs: ["JS", "#f1dd3f"],
  ts: ["TS", "#3b8eea"],
  tsx: ["⚛", "#4fc1ff"],
  jsx: ["⚛", "#4fc1ff"],
  css: ["#", "#42a5f5"],
  scss: ["#", "#f06292"],
  html: ["<>", "#e44d26"],
  json: ["{}", "#cbcb41"],
  md: ["M↓", "#519aba"],
  rs: ["R", "#dea584"],
  py: ["Py", "#3572a5"],
  toml: ["T", "#9c4221"],
  yml: ["Y", "#cb171e"],
  yaml: ["Y", "#cb171e"],
  png: ["▣", "#a074c4"],
  jpg: ["▣", "#a074c4"],
  svg: ["◇", "#ffb13b"],
  log: ["≡", "#8a8a8a"],
  txt: ["≡", "#8a8a8a"],
  sql: ["DB", "#dad8d8"],
};

function FileIcon({ name }: { name: string }) {
  const ext = name.includes(".") ? name.split(".").pop()!.toLowerCase() : "";
  const special = name === ".gitignore" ? ["◆", "#f14e32"] : name === "package.json" ? ["⬢", "#8bc34a"] : name.startsWith(".env") ? ["⚙", "#faf743"] : null;
  const [g, c] = special ?? FILE_ICON[ext] ?? ["▢", "#8a8a8a"];
  return (
    <span className="vi-file" style={{ color: c }}>
      {g}
    </span>
  );
}

const FOLDER_COLOR: Record<string, string> = {
  docs: "#42a5f5",
  src: "#4caf50",
  tests: "#26a69a",
  test: "#26a69a",
  styles: "#ab47bc",
  design: "#ec407a",
  scripts: "#ffa726",
  data: "#78909c",
  assets: "#ffca28",
  public: "#29b6f6",
  ".github": "#8a8a8a",
};

function Folder({ name, open }: { name: string; open: boolean }) {
  const c = FOLDER_COLOR[name] ?? "#dcb67a";
  return (
    <svg className="vi-folder" viewBox="0 0 16 16" aria-hidden="true">
      <path d={open ? "M1 4h5l1.5 1.5H15v1H4L2 13H1z M4 7h11.5L13 13H2z" : "M1 3h5l1.5 1.5H15V13H1z"} fill={c} />
    </svg>
  );
}

const Icon = {
  files: "M14 2H8L6 4v10h8zM6 4H4v12h8v-2",
  search: "M11 11l4 4M7 12a5 5 0 1 1 0-10 5 5 0 0 1 0 10",
  git: "M5 3v10M5 13a2 2 0 1 0 0 4M11 5a2 2 0 1 0 0-4 2 2 0 0 0 0 4zM11 5c0 4-6 3-6 7",
  run: "M4 3l9 5-9 5z",
  ext: "M2 9h5v5H2zM9 9h5v5H9zM2 2h5v5H2zM10 1l4 4-4 4-4-4z",
  account: "M8 8a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM2 15c0-3 3-5 6-5s6 2 6 5",
  gear: "M8 10.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5zM8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.5 1.5M11.5 11.5 13 13M3 13l1.5-1.5M11.5 4.5 13 3",
};

function Svg({ d, size = 22 }: { d: string; size?: number }) {
  return (
    <svg viewBox="0 0 16 16" width={size} height={size} aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" strokeLinecap="round">
      <path d={d} />
    </svg>
  );
}

export function VSCodeLogo({ size = 16 }: { size?: number }) {
  return (
    <svg viewBox="0 0 100 100" width={size} height={size} aria-hidden="true">
      <path fillRule="evenodd" fill="#0098ff" d="M74 4 33 41 13 26 4 30v40l9 4 20-15 41 37 22-10V14zM74 30v40L44 50z" />
    </svg>
  );
}

/* ---------- explorer tree ---------- */

interface Node {
  name: string;
  path: string;
  dir: boolean;
  kids: Node[];
}

function buildTree(paths: string[]): Node[] {
  const root: Node = { name: "", path: "", dir: true, kids: [] };
  for (const p of paths) {
    const parts = p.split("/");
    let cur = root;
    parts.forEach((part, i) => {
      const path = parts.slice(0, i + 1).join("/");
      const dir = i < parts.length - 1;
      let n = cur.kids.find((k) => k.name === part && k.dir === dir);
      if (!n) {
        n = { name: part, path, dir, kids: [] };
        cur.kids.push(n);
      }
      cur = n;
    });
  }
  const sort = (n: Node) => {
    n.kids.sort((a, b) => (a.dir === b.dir ? a.name.localeCompare(b.name) : a.dir ? -1 : 1));
    n.kids.forEach(sort);
  };
  sort(root);
  return root.kids;
}

function Tree({ nodes, depth, open, toggle, active, marks, onOpen }: { nodes: Node[]; depth: number; open: Set<string>; toggle: (p: string) => void; active: string | null; marks: Map<string, "U" | "M">; onOpen: (p: string) => void }) {
  return (
    <>
      {nodes.map((n) => {
        const isOpen = open.has(n.path);
        const mark = n.dir ? [...marks.keys()].some((m) => m.startsWith(n.path + "/")) : marks.get(n.path);
        return (
          <div key={n.path}>
            <button className={`vt-row${n.path === active ? " on" : ""}${mark ? (mark === "U" ? " untracked" : " modified") : ""}`} style={{ paddingLeft: 8 + depth * 12 }} onClick={() => (n.dir ? toggle(n.path) : onOpen(n.path))} title={n.path}>
              {n.dir ? <span className="vt-chev">{isOpen ? "⌄" : "›"}</span> : <span className="vt-chev" />}
              {n.dir ? <Folder name={n.name} open={isOpen} /> : <FileIcon name={n.name} />}
              <span className="vt-name">{n.name}</span>
              {!n.dir && typeof mark === "string" && <span className="vt-mark">{mark}</span>}
              {n.dir && mark && <span className="vt-dot">●</span>}
            </button>
            {n.dir && isOpen && <Tree nodes={n.kids} depth={depth + 1} open={open} toggle={toggle} active={active} marks={marks} onOpen={onOpen} />}
          </div>
        );
      })}
    </>
  );
}

/* ---------- the editor window ---------- */

export function VSCode({ codes, terms, projectId, projectName }: { codes: Code[]; terms: Term[]; projectId: string | null; projectName: string }) {
  // Project files for the explorer (refreshed when a new file shows up).
  const [files, setFiles] = useState<string[]>([]);
  const written = useMemo(() => codes.filter((c) => c.op !== "read").map((c) => c.path), [codes]);
  const missing = written.some((w) => !files.includes(w));
  useEffect(() => {
    if (!projectId) return;
    let alive = true;
    api<{ files: { path: string }[] }>("GET", `/api/projects/${projectId}/files`)
      .then((r) => alive && setFiles(r.files.map((f) => f.path.replace(/\\/g, "/"))))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [projectId, missing]);
  const allPaths = useMemo(() => [...new Set([...files, ...codes.map((c) => c.path)])], [files, codes]);
  const tree = useMemo(() => buildTree(allPaths), [allPaths]);
  const marks = useMemo(() => {
    const m = new Map<string, "U" | "M">();
    for (const c of codes) if (c.op !== "read") m.set(c.path, c.op === "write" && !m.has(c.path) ? "U" : "M");
    return m;
  }, [codes]);

  const [picked, setPicked] = useState<string | null>(null);
  const latest = codes[codes.length - 1];
  const path = picked ?? latest?.path ?? null;
  const ev = path ? [...codes].reverse().find((c) => c.path === path) : undefined;
  const tabs = useMemo(() => [...new Set([...codes].reverse().map((c) => c.path))].slice(0, 5).reverse(), [codes]);

  // Folders on the way to the files being worked on are open, like VS Code's "reveal".
  const [open, setOpen] = useState<Set<string>>(new Set());
  useEffect(() => {
    if (!path) return;
    setOpen((cur) => {
      const next = new Set(cur);
      const parts = path.split("/");
      for (let i = 1; i < parts.length; i++) next.add(parts.slice(0, i).join("/"));
      return next;
    });
  }, [path]);
  const toggle = (p: string) =>
    setOpen((cur) => {
      const n = new Set(cur);
      if (n.has(p)) n.delete(p);
      else n.add(p);
      return n;
    });

  // Typing: the part that changed since the previous version of this file is typed out.
  const prev = useMemo(() => {
    if (!ev) return null;
    const i = codes.lastIndexOf(ev);
    return [...codes.slice(0, i)].reverse().find((c) => c.path === ev.path)?.content ?? null;
  }, [codes, ev]);
  const content = ev?.content ?? "";
  const animate = !!ev && ev.op !== "read" && !picked && ev === latest;
  const [p, s] = prev !== null ? common(prev, content) : [0, 0];
  const regionEnd = content.length - s;
  const typeFrom = animate ? Math.max(p, regionEnd - 5000) : regionEnd;
  const toType = regionEnd - typeFrom;
  const typed = useTyping(toType, Math.min(6000, Math.max(400, toType * 3)), ev?.ts ?? 0);
  const caret = animate && typed < toType ? typeFrom + typed : animate ? regionEnd : -1;
  const text = caret >= 0 ? content.slice(0, caret) + content.slice(regionEnd) : content;
  const lines = useMemo(() => text.split("\n"), [text]);
  const lang = langOf(path ?? "");
  const toks = useMemo(() => highlight(lines, lang), [lines, lang]);
  const caretLine = caret >= 0 ? content.slice(0, caret).split("\n").length - 1 : -1;
  const caretCol = caret >= 0 ? caret - (content.lastIndexOf("\n", caret - 1) + 1) : -1;
  const hiFrom = ev?.from ?? null;
  const hiTo = ev?.to ?? null;
  const focusLine = caretLine >= 0 ? caretLine : hiFrom ? hiFrom - 1 : -1;
  const scroller = useRef<HTMLDivElement>(null);
  useEffect(() => {
    scroller.current?.querySelector<HTMLElement>(`[data-ln="${focusLine}"]`)?.scrollIntoView({ block: "center" });
  }, [focusLine, path]);

  const termRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    termRef.current?.scrollTo({ top: termRef.current.scrollHeight });
  }, [terms.length]);
  const [panel, setPanel] = useState(true);
  const failed = terms.filter((t) => !t.ok).length;

  return (
    <div className="vsc">
      <div className="vsc-titlebar">
        <VSCodeLogo />
        <div className="vsc-menu">
          {["File", "Edit", "Selection", "View", "Go", "Run", "Terminal", "Help"].map((m) => (
            <span key={m}>{m}</span>
          ))}
        </div>
        <span className="vsc-nav">←&nbsp;&nbsp;→</span>
        <div className="vsc-command">
          <Svg d={Icon.search} size={13} /> {projectName}
        </div>
        <span className="vsc-win">
          <i>—</i>
          <i>▢</i>
          <i>✕</i>
        </span>
      </div>
      <div className="vsc-main">
        <div className="vsc-activity">
          <span className="on" title="Explorer">
            <Svg d={Icon.files} />
          </span>
          <span title="Search">
            <Svg d={Icon.search} />
          </span>
          <span title="Source Control" className="badge-wrap">
            <Svg d={Icon.git} />
            {marks.size > 0 && <b className="act-badge">{marks.size}</b>}
          </span>
          <span title="Run and Debug">
            <Svg d={Icon.run} />
          </span>
          <span title="Extensions">
            <Svg d={Icon.ext} />
          </span>
          <span className="grow" />
          <span title="Accounts">
            <Svg d={Icon.account} />
          </span>
          <span title="Manage">
            <Svg d={Icon.gear} />
          </span>
        </div>
        <div className="vsc-side">
          <div className="vsc-side-h">
            EXPLORER <span>···</span>
          </div>
          <div className="vsc-root">⌄ {projectName.toUpperCase()}</div>
          <div className="vsc-tree">
            <Tree nodes={tree} depth={0} open={open} toggle={toggle} active={path} marks={marks} onOpen={(f) => setPicked(f === latest?.path ? null : f)} />
          </div>
          <div className="vsc-side-sec">› OUTLINE</div>
          <div className="vsc-side-sec">› TIMELINE</div>
        </div>
        <div className="vsc-editor">
          {path ? (
            <>
              <div className="vsc-tabs">
                {tabs.map((t) => (
                  <button key={t} className={`tab${t === path ? " on" : ""}`} onClick={() => setPicked(t === latest?.path ? null : t)} title={t}>
                    <FileIcon name={t.split("/").pop()!} /> {t.split("/").pop()}
                    {t === latest?.path && animate && typed < toType ? <span className="tab-dot">●</span> : <span className="tab-x">×</span>}
                  </button>
                ))}
                {picked && (
                  <button className="tab follow" onClick={() => setPicked(null)}>
                    ↩ Theo dõi file đang làm
                  </button>
                )}
              </div>
              <div className="vsc-crumbs">
                {path.split("/").map((seg, i, a) => (
                  <span key={i}>
                    {i === a.length - 1 ? <FileIcon name={seg} /> : null}
                    {seg}
                    {i < a.length - 1 ? <em> › </em> : null}
                  </span>
                ))}
                <span className="crumb-op">{ev?.op === "read" ? "· đang đọc" : ev?.op === "edit" ? "· đang sửa" : "· đang viết"}</span>
              </div>
              <div className="vsc-code" ref={scroller}>
                {lines.map((l, i) => (
                  <Line key={i} n={i} toks={toks[i]} hi={hiFrom != null && hiTo != null && i + 1 >= hiFrom && i + 1 <= hiTo} caret={i === caretLine ? caretCol : -1} text={l} />
                ))}
              </div>
            </>
          ) : (
            <div className="vsc-welcome">
              <VSCodeLogo size={220} />
              <div className="vsc-keys">
                <span>Show All Commands</span>
                <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>
                <span>Go to File</span>
                <kbd>Ctrl</kbd>+<kbd>P</kbd>
                <span>Toggle Terminal</span>
                <kbd>Ctrl</kbd>+<kbd>`</kbd>
              </div>
            </div>
          )}
          <div className={`vsc-panel${panel ? "" : " closed"}`}>
            <div className="vsc-panel-tabs">
              <span>
                PROBLEMS {failed > 0 && <b className="act-badge">{failed}</b>}
              </span>
              <span>OUTPUT</span>
              <span>DEBUG CONSOLE</span>
              <button className="on" onClick={() => setPanel((v) => !v)}>
                TERMINAL
              </button>
              <span>PORTS</span>
              <span className="grow" />
              <span className="term-kind">⊞ powershell</span>
            </div>
            {panel && (
              <div className="vsc-term-body" ref={termRef}>
                {terms.length === 0 && <div className="prompt">PS D:\{projectName}&gt; <span className="caret" /></div>}
                {terms.slice(-6).map((t, i, a) => (
                  <div key={t.ts}>
                    <div className="prompt">
                      PS D:\{projectName}&gt; <span className="cmd">{t.command}</span>
                    </div>
                    <pre className={t.ok ? "" : "err"}>{t.output.replace(/^exit code: \d+\s*/, "").slice(-3000)}</pre>
                    {i === a.length - 1 && (
                      <div className="prompt">
                        PS D:\{projectName}&gt; <span className="caret" />
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
      <div className="vsc-status">
        <span className="remote">⟩⟨</span>
        <span>⎇ main</span>
        <span>
          ⊗ {failed} ⚠ 0
        </span>
        <span className="grow" />
        <span>{caretLine >= 0 ? `Ln ${caretLine + 1}, Col ${caretCol + 1}` : path ? `${lines.length} dòng` : ""}</span>
        <span>Spaces: 2</span>
        <span>UTF-8</span>
        <span>CRLF</span>
        <span>{"{ }"} {lang === "text" ? "Plain Text" : lang.toUpperCase()}</span>
        <span>🔔</span>
      </div>
    </div>
  );
}

const Line = memo(function Line({ n, toks, hi, caret, text }: { n: number; toks: Tok[] | undefined; hi: boolean; caret: number; text: string }) {
  const parts = toks ?? [["", text]];
  let body: React.ReactNode;
  if (caret < 0) {
    body = parts.map(([c, t], i) => (
      <span key={i} className={c ? `t-${c}` : undefined}>
        {t}
      </span>
    ));
  } else {
    let col = 0;
    const out: React.ReactNode[] = [];
    parts.forEach(([c, t], i) => {
      const cls = c ? `t-${c}` : undefined;
      if (caret >= col && caret < col + t.length) {
        out.push(
          <span key={`${i}a`} className={cls}>
            {t.slice(0, caret - col)}
          </span>,
          <span key="caret" className="caret" />,
          <span key={`${i}b`} className={cls}>
            {t.slice(caret - col)}
          </span>,
        );
      } else
        out.push(
          <span key={i} className={cls}>
            {t}
          </span>,
        );
      col += t.length;
    });
    if (caret >= col) out.push(<span key="caret" className="caret" />);
    body = out;
  }
  return (
    <div className={`ln${hi ? " hi" : ""}${caret >= 0 ? " cur" : ""}`} data-ln={n}>
      <span className="ln-n">{n + 1}</span>
      <span className="ln-t">{body}</span>
    </div>
  );
});
