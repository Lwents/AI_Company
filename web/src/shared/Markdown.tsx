import type { ReactNode } from "react";

/**
 * Small Markdown renderer for what employees write: headings, bullet / numbered lists (nested by
 * indent), **bold**, *italic*, `code`, ``` blocks and paragraphs. Builds React elements only
 * (never innerHTML), so model text cannot inject markup.
 */
export function Markdown({ text, className }: { text: string; className?: string }) {
  return <div className={"md " + (className ?? "")}>{blocks(text)}</div>;
}

/** `start`: the number written in the source, so "2." after a blank line still shows 2. */
type Item = { depth: number; ordered: boolean; start: number; text: string };

function blocks(src: string): ReactNode[] {
  const out: ReactNode[] = [];
  const lines = src.replace(/\r\n?/g, "\n").split("\n");
  let para: string[] = [];
  let list: Item[] = [];
  const flushPara = () => {
    if (para.length) out.push(<p key={out.length}>{inline(para.join("\n"))}</p>);
    para = [];
  };
  const flushList = () => {
    if (list.length) out.push(<div key={out.length}>{renderList(list, 0, { i: 0 })}</div>);
    list = [];
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (/^\s*```/.test(line)) {
      flushPara();
      flushList();
      const code: string[] = [];
      while (++i < lines.length && !/^\s*```/.test(lines[i])) code.push(lines[i]);
      out.push(
        <pre key={out.length} className="code">
          {code.join("\n")}
        </pre>,
      );
      continue;
    }
    const h = /^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$/.exec(line);
    if (h) {
      flushPara();
      flushList();
      out.push(
        <div key={out.length} className={"md-h md-h" + Math.min(h[1].length, 3)}>
          {inline(h[2])}
        </div>,
      );
      continue;
    }
    const li = /^(\s*)([-*+•]|\d+[.)])\s+(.*)$/.exec(line);
    if (li) {
      flushPara();
      const indent = li[1].replace(/\t/g, "  ").length;
      list.push({ depth: Math.floor(indent / 2), ordered: /\d/.test(li[2]), start: parseInt(li[2], 10) || 1, text: li[3] });
      continue;
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      flushPara();
      flushList();
      out.push(<hr key={out.length} />);
      continue;
    }
    if (!line.trim()) {
      flushPara();
      flushList();
      continue;
    }
    // A plain line right after a list item continues that item.
    if (list.length && /^\s+\S/.test(line)) {
      list[list.length - 1].text += " " + line.trim();
      continue;
    }
    flushList();
    para.push(line);
  }
  flushPara();
  flushList();
  return out;
}

/** Renders items starting at pos.i with depth >= `depth`; deeper items become nested lists. */
function renderList(items: Item[], depth: number, pos: { i: number }): ReactNode {
  const { ordered, start } = items[pos.i];
  const kids: ReactNode[] = [];
  while (pos.i < items.length && items[pos.i].depth >= depth) {
    const it = items[pos.i++];
    const sub = pos.i < items.length && items[pos.i].depth > it.depth ? renderList(items, items[pos.i].depth, pos) : null;
    kids.push(
      <li key={kids.length}>
        {inline(it.text)}
        {sub}
      </li>,
    );
  }
  return ordered ? <ol start={start}>{kids}</ol> : <ul>{kids}</ul>;
}

/**
 * Inline: `code`, **bold** / __bold__, *italic*. Code spans are cut out first (stars inside code
 * such as `ui/**` must not open bold; bold may wrap code). Unclosed markers stay as text.
 */
function inline(s: string): ReactNode[] {
  const codes: string[] = [];
  const masked = s.replace(/`([^`\n]+)`/g, (_, c: string) => `${codes.push(c) - 1}`);
  return emphasis(masked, codes);
}

function emphasis(s: string, codes: string[]): ReactNode[] {
  const out: ReactNode[] = [];
  const re = /(\*\*|__)(?=\S)(.+?)(?<=\S)\1|\*(?=[^\s*])(.+?)(?<=[^\s*])\*/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(s))) {
    if (m.index > last) out.push(...unmask(s.slice(last, m.index), codes));
    if (m[1]) out.push(<b key={out.length}>{emphasis(m[2], codes)}</b>);
    else out.push(<i key={out.length}>{emphasis(m[3], codes)}</i>);
    last = re.lastIndex;
  }
  if (last < s.length) out.push(...unmask(s.slice(last), codes));
  return out;
}

/** Puts the cut-out code spans back as <code>. */
function unmask(s: string, codes: string[]): ReactNode[] {
  return s.split(/(\d+)/).map((part, i) => (i % 2 ? <code key={"c" + part}>{codes[Number(part)]}</code> : part));
}
