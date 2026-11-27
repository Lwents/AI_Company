/** Tiny syntax highlighter for the screen view (VS Code Dark+ colours). Line based, good enough to look real. */
export type Tok = [cls: string, text: string];
export type Lang = "js" | "css" | "html" | "rust" | "py" | "json" | "md" | "sh" | "text";

export function langOf(path: string): Lang {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  if (["js", "mjs", "cjs", "ts", "tsx", "jsx"].includes(ext)) return "js";
  if (["css", "scss", "less"].includes(ext)) return "css";
  if (["html", "htm", "vue", "svg", "xml"].includes(ext)) return "html";
  if (ext === "rs") return "rust";
  if (ext === "py") return "py";
  if (ext === "json") return "json";
  if (["md", "markdown"].includes(ext)) return "md";
  if (["sh", "ps1", "bat", "cmd", "toml", "yml", "yaml", "env"].includes(ext)) return "sh";
  return "text";
}

const KW: Record<string, Set<string>> = {
  js: new Set(
    "const let var function return if else for while do switch case break continue new class extends import from export default async await try catch finally throw typeof instanceof in of this null undefined true false void yield static get set interface type enum implements public private protected readonly as".split(
      " ",
    ),
  ),
  rust: new Set("fn let mut pub use mod struct enum impl trait for in if else match return loop while async await move ref self Self crate super where as const static true false Some None Ok Err dyn type unsafe".split(" ")),
  py: new Set("def class return if elif else for while in import from as try except finally with lambda yield None True False and or not is pass break continue raise global async await self".split(" ")),
  css: new Set([]),
};
const CONTROL = new Set("return if else for while do switch case break continue try catch finally throw await yield match loop import from export".split(" "));

const RE: Record<string, RegExp> = {
  js: /(\/\/.*$)|(\/\*.*?\*\/)|("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`)|(\b\d[\d_.]*(?:px|rem|em|ms|s|%)?\b)|([A-Za-z_$][\w$]*)(?=\s*\()|([A-Za-z_$][\w$]*)|([{}()[\];,.<>=+\-*/!&|?:]+)/g,
  rust: /(\/\/.*$)|(\/\*.*?\*\/)|("(?:\\.|[^"\\])*")|(\b\d[\d_.]*\b)|([A-Za-z_][\w]*!?)(?=\s*\()|([A-Za-z_][\w]*)|([{}()[\];,.<>=+\-*/!&|?:#]+)/g,
  py: /(#.*$)|(\/\*\*\/)|("""|'''|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')|(\b\d[\d_.]*\b)|([A-Za-z_][\w]*)(?=\s*\()|([A-Za-z_][\w]*)|([{}()[\];,.<>=+\-*/!&|?:@]+)/g,
  css: /(\/\*.*?\*\/)|(--[\w-]+)|("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')|(#[0-9a-fA-F]{3,8}\b|\b\d[\d.]*(?:px|rem|em|vh|vw|ms|s|%|fr|deg)?\b)|([\w-]+)(?=\s*:)|([.#]?[\w-]+)|([{}();,:>+~*]+)/g,
  json: /("(?:\\.|[^"\\])*")(?=\s*:)|("(?:\\.|[^"\\])*")|(\b-?\d[\d.eE+-]*\b)|(\btrue\b|\bfalse\b|\bnull\b)|([{}[\],:]+)/g,
};

function codeLine(line: string, lang: "js" | "rust" | "py" | "css"): Tok[] {
  const out: Tok[] = [];
  const re = new RegExp(RE[lang].source, "g");
  let last = 0;
  for (let m = re.exec(line); m; m = re.exec(line)) {
    if (m.index > last) out.push(["", line.slice(last, m.index)]);
    const [t, com, com2, str, num, fn, word, punct] = m;
    if (lang === "css") {
      // css groups: comment, variable, string, number/colour, property, selector, punct
      const cls = m[1] ? "c" : m[2] ? "v" : m[3] ? "s" : m[4] ? "n" : m[5] ? "p" : m[6] ? "t" : "o";
      out.push([cls, t]);
    } else if (com || com2) out.push(["c", t]);
    else if (str) out.push(["s", t]);
    else if (num) out.push(["n", t]);
    else if (fn) out.push([KW[lang].has(fn) ? (CONTROL.has(fn) ? "k2" : "k") : "f", t]);
    else if (word) out.push([KW[lang].has(word) ? (CONTROL.has(word) ? "k2" : "k") : /^[A-Z]/.test(word) ? "t" : "v", t]);
    else if (punct) out.push(["o", t]);
    else out.push(["", t]);
    last = m.index + t.length;
    if (t.length === 0) re.lastIndex++;
  }
  if (last < line.length) out.push(["", line.slice(last)]);
  return out;
}

function htmlLine(line: string): Tok[] {
  const out: Tok[] = [];
  const re = /(<!--.*?-->)|(<\/?)([\w-]+)|([\w-:@]+)(=)("[^"]*"|'[^']*')|(\/?>)|("[^"]*")/g;
  let last = 0;
  for (let m = re.exec(line); m; m = re.exec(line)) {
    if (m.index > last) out.push(["", line.slice(last, m.index)]);
    if (m[1]) out.push(["c", m[1]]);
    else if (m[2]) {
      out.push(["o", m[2]]);
      out.push(["k", m[3]]);
    } else if (m[4]) {
      out.push(["v", m[4]]);
      out.push(["o", m[5]]);
      out.push(["s", m[6]]);
    } else if (m[7]) out.push(["o", m[7]]);
    else if (m[8]) out.push(["s", m[8]]);
    last = m.index + m[0].length;
  }
  if (last < line.length) out.push(["", line.slice(last)]);
  return out;
}

function mdLine(line: string): Tok[] {
  if (/^\s*#{1,6}\s/.test(line)) return [["k", line]];
  if (/^\s*```/.test(line)) return [["c", line]];
  const out: Tok[] = [];
  const re = /(`[^`]+`)|(\*\*[^*]+\*\*)|(^\s*(?:[-*+]|\d+\.)\s)|(\[[^\]]*\]\([^)]*\))/g;
  let last = 0;
  for (let m = re.exec(line); m; m = re.exec(line)) {
    if (m.index > last) out.push(["", line.slice(last, m.index)]);
    out.push([m[1] ? "s" : m[2] ? "b" : m[3] ? "n" : "f", m[0]]);
    last = m.index + m[0].length;
  }
  if (last < line.length) out.push(["", line.slice(last)]);
  return out;
}

function jsonLine(line: string): Tok[] {
  const out: Tok[] = [];
  const re = new RegExp(RE.json.source, "g");
  let last = 0;
  for (let m = re.exec(line); m; m = re.exec(line)) {
    if (m.index > last) out.push(["", line.slice(last, m.index)]);
    out.push([m[1] ? "v" : m[2] ? "s" : m[3] ? "n" : m[4] ? "k" : "o", m[0]]);
    last = m.index + m[0].length;
  }
  if (last < line.length) out.push(["", line.slice(last)]);
  return out;
}

/** Tokens for every line; multi-line block comments are carried across lines. */
export function highlight(lines: string[], lang: Lang): Tok[][] {
  if (lines.length > 4000) return lines.map((l) => [["", l]]);
  let inBlock = false;
  return lines.map((line) => {
    if (lang === "text") return [["", line]];
    if (lang === "md") return mdLine(line);
    if (lang === "json") return jsonLine(line);
    if (lang === "sh") return /^\s*#/.test(line) ? [["c", line]] : [["", line]];
    // Block comments spanning lines (/* … */, <!-- … -->).
    const open = lang === "html" ? "<!--" : "/*";
    const close = lang === "html" ? "-->" : "*/";
    if (inBlock) {
      const end = line.indexOf(close);
      if (end < 0) return [["c", line]];
      inBlock = false;
      const head: Tok = ["c", line.slice(0, end + close.length)];
      const rest = line.slice(end + close.length);
      return [head, ...(lang === "html" ? htmlLine(rest) : codeLine(rest, lang as "js"))];
    }
    const start = line.indexOf(open);
    if (start >= 0 && line.indexOf(close, start + open.length) < 0 && lang !== "py") {
      inBlock = true;
      const before = line.slice(0, start);
      return [...(lang === "html" ? htmlLine(before) : codeLine(before, lang as "js")), ["c", line.slice(start)]];
    }
    return lang === "html" ? htmlLine(line) : codeLine(line, lang as "js");
  });
}
