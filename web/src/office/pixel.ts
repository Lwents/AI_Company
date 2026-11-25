/**
 * Pixel-art toolkit for the office: palette, character sprites built from text art
 * (auto-outlined, cached), and small furniture painters. All coordinates here are
 * in *pixel units* of the low-resolution office canvas (world / 2).
 */

// Sweetie 16 palette + a few wood tones.
export const C = {
  night: "#1a1c2c",
  plum: "#5d275d",
  red: "#b13e53",
  orange: "#ef7d57",
  yellow: "#ffcd75",
  lime: "#a7f070",
  green: "#38b764",
  teal: "#257179",
  navy: "#29366f",
  blue: "#3b5dc9",
  sky: "#41a6f6",
  cyan: "#73eff7",
  white: "#f4f4f4",
  silver: "#94b0c2",
  slate: "#566c86",
  ink: "#333c57",
  outline: "#14121f",
  wood1: "#5e3a26",
  wood2: "#8b5a3c",
  wood3: "#a8714a",
  wood4: "#c48a5a",
};

export type Ctx = CanvasRenderingContext2D;

export function rect(ctx: Ctx, x: number, y: number, w: number, h: number, color: string) {
  ctx.fillStyle = color;
  ctx.fillRect(Math.round(x), Math.round(y), Math.round(w), Math.round(h));
}

export function px(ctx: Ctx, x: number, y: number, color: string) {
  ctx.fillStyle = color;
  ctx.fillRect(Math.round(x), Math.round(y), 1, 1);
}

/** Filled pixel ellipse (row by row, no anti-aliasing). */
export function ellipse(ctx: Ctx, cx: number, cy: number, rx: number, ry: number, color: string) {
  ctx.fillStyle = color;
  for (let dy = -ry; dy <= ry; dy++) {
    const half = Math.round(rx * Math.sqrt(Math.max(0, 1 - (dy * dy) / (ry * ry))));
    ctx.fillRect(Math.round(cx - half), Math.round(cy + dy), half * 2, 1);
  }
}

export function shade(hex: string, amt: number): string {
  const n = parseInt(hex.slice(1), 16);
  let r = (n >> 16) & 255;
  let g = (n >> 8) & 255;
  let b = n & 255;
  const f = (v: number) => Math.max(0, Math.min(255, Math.round(amt < 0 ? v * (1 + amt) : v + (255 - v) * amt)));
  r = f(r);
  g = f(g);
  b = f(b);
  return `#${((1 << 24) | (r << 16) | (g << 8) | b).toString(16).slice(1)}`;
}

// ---------------------------------------------------------------- characters

/*
 * 12×18 front-facing chibi. Keys:
 *  h hair  H hair highlight  s skin  d skin shade  r blush  e eye  m mouth
 *  c shirt  C shirt shade  t tie/collar  p pants  f shoes   . empty
 */
const HEAD = [
  "...hhhhhh...",
  "..hHHhhhhh..",
  ".hhhhhhhhhh.",
  ".hhsssssshh.",
  ".hssssssssh.",
  ".ssessssess.",
  ".ssessssess.",
  ".srssssssrs.",
  "..sssmmsss..",
  "...dssssd...",
];
const BODY = ["..cccttccc..", ".Ccccttcccc.", ".Ccccttcccc.", ".sCccccccCs.", "..pppppppp.."];
const BODY_TYPE = ["..cccttccc..", ".Ccccttcccc.", ".sccccccccs.", "..CcccccccC.", "..pppppppp.."];
const LEGS = {
  stand: ["..ppp..ppp..", "..ppp..ppp..", "..fff..fff.."],
  walkA: ["..ppp..ppp..", "..ppp..fff..", "..fff......."],
  walkB: ["..ppp..ppp..", "..fff..ppp..", ".......fff.."],
};

export type Frame = "stand" | "walkA" | "walkB" | "type";

export interface Look {
  hair: string;
  skin: string;
  shirt: string;
  pants: string;
  style: number; // 0 short, 1 long, 2 spiky, 3 cap
  tie: string;
  glasses: boolean;
}

const HAIRS = ["#2b2016", "#4a3020", "#151515", "#7a4a26", "#d9a55a", "#5a5a66", "#8e3b2e"];
const SKINS = ["#f5d0b0", "#e8b48f", "#d39a72", "#a8704b", "#f2c8a0"];
const PANTS = ["#29366f", "#333c57", "#3d3d3d", "#566c86", "#5e3a26"];

function hash(s: string) {
  let h = 2166136261;
  for (const c of s) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  return Math.abs(h);
}

export function lookFor(id: string, name: string, color: string, role: string): Look {
  const h = hash(id + "|" + name);
  return {
    hair: HAIRS[h % HAIRS.length],
    skin: SKINS[(h >> 4) % SKINS.length],
    shirt: color,
    pants: PANTS[(h >> 8) % PANTS.length],
    style: role === "ceo" ? 0 : (h >> 12) % 4,
    tie: role === "ceo" ? C.red : shade(color, 0.45),
    glasses: role === "ceo" || role === "architect" || (h >> 16) % 4 === 0,
  };
}

function buildRows(look: Look, frame: Frame, facing: 1 | -1): string[] {
  const head = HEAD.map((r) => r.split(""));
  // Eyes follow the facing direction.
  if (facing !== 1 || frame === "walkA" || frame === "walkB") {
    for (const row of [5, 6]) {
      const r = head[row];
      r[3] = r[8] = "s";
      const shift = facing === 1 ? 1 : -1;
      r[3 + shift] = "e";
      r[8 + shift] = "e";
    }
  }
  if (look.glasses) {
    for (const col of [2, 3, 4, 7, 8, 9]) if (head[5][col] === "s") head[5][col] = "g";
    head[5][5] = head[5][6] = "g";
  }
  switch (look.style) {
    case 1: // long hair framing the face
      for (let r = 3; r <= 8; r++) {
        head[r][1] = "h";
        head[r][10] = "h";
        if (r >= 4) {
          head[r][0] = "h";
          head[r][11] = "h";
        }
      }
      break;
    case 2: // spiky
      head[0] = ".h.hhhhhh.h.".split("");
      head[1] = "hhhHHhhhhhhh".split("");
      break;
    case 3: // cap
      head[0] = "...cccccc...".split("");
      head[1] = "..cCCccccc..".split("");
      head[2] = ".ccccccccccc".split("");
      head[3] = ".hhsssssshh.".split("");
      break;
  }
  const body = frame === "type" ? BODY_TYPE : BODY;
  const legs = frame === "walkA" ? LEGS.walkA : frame === "walkB" ? LEGS.walkB : LEGS.stand;
  return [...head.map((r) => r.join("")), ...body, ...legs];
}

const cache = new Map<string, HTMLCanvasElement>();

/** Returns a 14×20 sprite (12×18 + 1px outline). Feet are at the bottom-centre. */
export function characterSprite(look: Look, frame: Frame, facing: 1 | -1): HTMLCanvasElement {
  const key = `${look.hair}${look.skin}${look.shirt}${look.pants}${look.style}${look.tie}${look.glasses}|${frame}|${facing}`;
  const hit = cache.get(key);
  if (hit) return hit;
  const rows = buildRows(look, frame, facing);
  const W = 14;
  const H = rows.length + 2;
  const colors: Record<string, string> = {
    h: look.hair,
    H: shade(look.hair, 0.3),
    s: look.skin,
    d: shade(look.skin, -0.15),
    r: "#e8908a",
    e: "#1b1b24",
    m: "#9a4a3a",
    g: "#1b1b24",
    c: look.shirt,
    C: shade(look.shirt, -0.25),
    t: look.tie,
    p: look.pants,
    f: "#1e1e26",
  };
  const grid: (string | null)[][] = Array.from({ length: H }, () => Array(W).fill(null));
  rows.forEach((row, y) => {
    for (let x = 0; x < row.length; x++) {
      const k = row[x];
      if (k !== "." && colors[k]) grid[y + 1][x + 1] = colors[k];
    }
  });
  const canvas = document.createElement("canvas");
  canvas.width = W;
  canvas.height = H;
  const ctx = canvas.getContext("2d")!;
  // Outline: any empty pixel touching a filled one.
  for (let y = 0; y < H; y++)
    for (let x = 0; x < W; x++) {
      if (grid[y][x]) continue;
      const near = [
        [1, 0],
        [-1, 0],
        [0, 1],
        [0, -1],
      ].some(([dx, dy]) => grid[y + dy]?.[x + dx]);
      if (near) px(ctx, x, y, C.outline);
    }
  for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) if (grid[y][x]) px(ctx, x, y, grid[y][x]!);
  if (facing === -1) {
    const flip = document.createElement("canvas");
    flip.width = W;
    flip.height = H;
    const f = flip.getContext("2d")!;
    f.translate(W, 0);
    f.scale(-1, 1);
    f.drawImage(canvas, 0, 0);
    cache.set(key, flip);
    return flip;
  }
  cache.set(key, canvas);
  return canvas;
}

// ---------------------------------------------------------------- furniture

export function plant(ctx: Ctx, x: number, y: number) {
  // Pot (feet at x,y).
  rect(ctx, x - 4, y - 5, 8, 5, C.orange);
  rect(ctx, x - 4, y - 5, 8, 1, shade(C.orange, 0.25));
  rect(ctx, x - 3, y - 1, 6, 1, C.red);
  // Leaves.
  const g1 = C.green;
  const g2 = C.lime;
  const g0 = C.teal;
  rect(ctx, x - 5, y - 11, 10, 6, g1);
  rect(ctx, x - 3, y - 14, 6, 3, g1);
  rect(ctx, x - 7, y - 9, 2, 3, g0);
  rect(ctx, x + 5, y - 9, 2, 3, g0);
  px(ctx, x - 2, y - 13, g2);
  px(ctx, x + 1, y - 10, g2);
  px(ctx, x - 4, y - 9, g2);
  px(ctx, x + 3, y - 12, g2);
  rect(ctx, x - 5, y - 6, 10, 1, g0);
}

export function windowPane(ctx: Ctx, x: number, y: number, w: number, h: number, t: number) {
  rect(ctx, x - 1, y - 1, w + 2, h + 2, C.silver);
  rect(ctx, x, y, w, h, C.navy);
  rect(ctx, x, y + Math.floor(h / 2), w, Math.ceil(h / 2), C.blue);
  // A few twinkling stars / city lights.
  for (let i = 0; i < w; i += 5) {
    const on = Math.sin(t / 700 + i * 1.7) > 0.2;
    if (on) px(ctx, x + i + 1, y + 1 + (i % 3), C.yellow);
  }
  rect(ctx, x + Math.floor(w / 2), y, 1, h, C.silver);
  rect(ctx, x, y + Math.floor(h / 2), w, 1, C.silver);
  px(ctx, x + 1, y + h - 2, C.cyan);
}

export function desk(ctx: Ctx, cx: number, cy: number, wide: boolean, glow: boolean, mug: string, t: number) {
  const w = wide ? 40 : 30;
  const x = Math.round(cx - w / 2);
  const top = wide ? C.wood3 : C.silver;
  const front = wide ? C.wood2 : C.slate;
  const dark = wide ? C.wood1 : C.ink;
  // Monitor (screen faces the worker, tilted so we can see it).
  const mx = Math.round(cx + (wide ? 7 : 4));
  const my = cy - 15;
  rect(ctx, mx - 1, my - 1, 13, 10, C.outline);
  rect(ctx, mx, my, 11, 8, C.ink);
  rect(ctx, mx + 1, my + 1, 9, 6, glow ? C.night : "#20263a");
  if (glow) {
    // Scrolling code lines.
    for (let i = 0; i < 3; i++) {
      const len = 2 + ((Math.floor(t / 180) + i * 3) % 6);
      rect(ctx, mx + 2, my + 2 + i * 2, len, 1, i === 1 ? C.lime : C.cyan);
    }
  }
  rect(ctx, mx + 4, my + 8, 3, 2, C.ink);
  // Desk top + front panel.
  rect(ctx, x - 1, cy - 7, w + 2, 15, C.outline);
  rect(ctx, x, cy - 6, w, 4, top);
  rect(ctx, x, cy - 6, w, 1, shade(top, 0.25));
  rect(ctx, x, cy - 2, w, 9, front);
  rect(ctx, x, cy + 6, w, 1, dark);
  if (wide) {
    // Drawers.
    rect(ctx, x + 3, cy, 8, 5, dark);
    rect(ctx, x + w - 11, cy, 8, 5, dark);
    px(ctx, x + 7, cy + 2, C.yellow);
    px(ctx, x + w - 7, cy + 2, C.yellow);
  } else {
    rect(ctx, x + 2, cy + 1, w - 4, 1, dark);
  }
  // Keyboard and mug on the desk top.
  rect(ctx, cx - 5, cy - 5, 9, 2, C.ink);
  rect(ctx, cx - 4, cy - 5, 7, 1, C.silver);
  rect(ctx, x + 2, cy - 9, 4, 4, mug);
  rect(ctx, x + 2, cy - 9, 4, 1, shade(mug, 0.35));
  px(ctx, x + 6, cy - 8, mug);
}

export function chair(ctx: Ctx, cx: number, cy: number) {
  rect(ctx, cx - 7, cy - 13, 14, 9, C.outline);
  rect(ctx, cx - 6, cy - 12, 12, 7, C.ink);
  rect(ctx, cx - 6, cy - 12, 12, 1, C.slate);
}

export function bookshelf(ctx: Ctx, x: number, y: number) {
  rect(ctx, x - 1, y - 1, 32, 14, C.outline);
  rect(ctx, x, y, 30, 12, C.wood2);
  const books = [C.red, C.sky, C.green, C.yellow, C.plum, C.orange, C.cyan, C.white];
  for (let row = 0; row < 2; row++) {
    rect(ctx, x + 1, y + 1 + row * 6, 28, 5, C.wood1);
    for (let i = 0; i < 9; i++) {
      const h = 3 + ((i + row) % 2);
      rect(ctx, x + 2 + i * 3, y + 6 + row * 6 - h, 2, h, books[(i + row * 3) % books.length]);
    }
  }
}

export function serverRack(ctx: Ctx, x: number, y: number, t: number) {
  rect(ctx, x - 1, y - 1, 14, 30, C.outline);
  rect(ctx, x, y, 12, 28, C.ink);
  for (let i = 0; i < 6; i++) {
    rect(ctx, x + 1, y + 1 + i * 4.5, 10, 3, "#262b40");
    const on = Math.sin(t / 150 + i * 2.1) > 0;
    px(ctx, x + 9, y + 2 + i * 4.5, on ? C.lime : C.teal);
    px(ctx, x + 7, y + 2 + i * 4.5, Math.sin(t / 90 + i) > 0.6 ? C.orange : C.ink);
  }
}

export function sofa(ctx: Ctx, x: number, y: number, w: number) {
  rect(ctx, x - 1, y - 1, w + 2, 16, C.outline);
  rect(ctx, x, y, w, 7, C.navy);
  rect(ctx, x, y + 7, w, 7, C.blue);
  rect(ctx, x, y + 7, w, 1, shade(C.blue, 0.25));
  rect(ctx, x, y, 4, 14, C.navy);
  rect(ctx, x + w - 4, y, 4, 14, C.navy);
  px(ctx, x + Math.floor(w / 2), y + 9, C.navy);
}

export function coffeeCorner(ctx: Ctx, x: number, y: number, t: number) {
  // Counter.
  rect(ctx, x - 1, y - 1, 52, 13, C.outline);
  rect(ctx, x, y, 50, 4, C.silver);
  rect(ctx, x, y + 4, 50, 7, C.slate);
  // Coffee machine with steam.
  const mx = x + 30;
  rect(ctx, mx - 1, y - 13, 12, 14, C.outline);
  rect(ctx, mx, y - 12, 10, 12, "#2b2b33");
  rect(ctx, mx + 2, y - 10, 6, 3, C.ink);
  px(ctx, mx + 3, y - 9, C.red);
  rect(ctx, mx + 3, y - 4, 4, 3, C.white);
  for (let i = 0; i < 3; i++) {
    const sy = y - 15 - ((t / 120 + i * 4) % 10);
    px(ctx, mx + 4 + Math.round(Math.sin(t / 200 + i)), sy, "rgba(244,244,244,0.7)");
  }
  // Cups.
  rect(ctx, x + 6, y - 3, 3, 3, C.white);
  rect(ctx, x + 11, y - 3, 3, 3, C.orange);
}

export function fridge(ctx: Ctx, x: number, y: number) {
  rect(ctx, x - 1, y - 1, 15, 30, C.outline);
  rect(ctx, x, y, 13, 28, C.white);
  rect(ctx, x, y + 10, 13, 1, C.silver);
  rect(ctx, x + 10, y + 3, 1, 5, C.silver);
  rect(ctx, x + 10, y + 13, 1, 8, C.silver);
  px(ctx, x + 3, y + 4, C.red);
  px(ctx, x + 5, y + 5, C.yellow);
}

export function waterCooler(ctx: Ctx, x: number, y: number) {
  rect(ctx, x - 1, y - 1, 10, 22, C.outline);
  rect(ctx, x, y + 8, 8, 12, C.white);
  rect(ctx, x + 1, y, 6, 8, C.sky);
  rect(ctx, x + 2, y + 1, 2, 5, C.cyan);
  px(ctx, x + 2, y + 11, C.sky);
  px(ctx, x + 5, y + 11, C.red);
}

export function printer(ctx: Ctx, x: number, y: number) {
  rect(ctx, x - 1, y - 1, 18, 12, C.outline);
  rect(ctx, x, y, 16, 10, C.silver);
  rect(ctx, x + 2, y - 3, 12, 3, C.white);
  rect(ctx, x + 2, y + 3, 12, 2, C.ink);
  px(ctx, x + 13, y + 7, C.lime);
}

export function painting(ctx: Ctx, x: number, y: number, w: number, h: number) {
  rect(ctx, x - 1, y - 1, w + 2, h + 2, C.outline);
  rect(ctx, x, y, w, h, C.yellow);
  rect(ctx, x + 1, y + 1, w - 2, h - 2, C.sky);
  rect(ctx, x + 1, y + h - 4, w - 2, 3, C.green);
  rect(ctx, x + w - 6, y + 2, 3, 3, C.yellow);
  rect(ctx, x + 3, y + h - 7, 4, 3, C.teal);
}

export function globe(ctx: Ctx, x: number, y: number, t: number) {
  rect(ctx, x - 1, y + 7, 6, 2, C.wood1);
  rect(ctx, x + 1, y + 5, 2, 2, C.wood2);
  ellipse(ctx, x + 2, y + 2, 4, 4, C.outline);
  ellipse(ctx, x + 2, y + 2, 3, 3, C.sky);
  const o = Math.floor(t / 400) % 4;
  rect(ctx, x + o - 1, y, 2, 2, C.green);
  px(ctx, x + ((o + 2) % 4), y + 3, C.green);
}

export function receptionCounter(ctx: Ctx, cx: number, cy: number) {
  const x = cx - 18;
  rect(ctx, x - 1, cy - 8, 38, 16, C.outline);
  rect(ctx, x, cy - 7, 36, 5, C.wood4);
  rect(ctx, x, cy - 7, 36, 1, shade(C.wood4, 0.3));
  rect(ctx, x, cy - 2, 36, 9, C.wood2);
  rect(ctx, x + 4, cy, 28, 4, C.wood3);
  // Bell + flowers.
  rect(ctx, x + 28, cy - 10, 4, 3, C.yellow);
  px(ctx, x + 29, cy - 11, C.yellow);
  rect(ctx, x + 3, cy - 11, 3, 4, C.white);
  px(ctx, x + 3, cy - 12, C.red);
  px(ctx, x + 5, cy - 13, C.orange);
}

export function easel(ctx: Ctx, x: number, y: number) {
  rect(ctx, x + 1, y + 10, 1, 8, C.wood2);
  rect(ctx, x + 10, y + 10, 1, 8, C.wood2);
  rect(ctx, x - 1, y - 1, 14, 13, C.outline);
  rect(ctx, x, y, 12, 11, C.white);
  rect(ctx, x + 2, y + 2, 4, 4, C.orange);
  rect(ctx, x + 6, y + 5, 4, 4, C.sky);
  rect(ctx, x + 2, y + 8, 8, 1, C.plum);
}

export function swatches(ctx: Ctx, x: number, y: number) {
  const cols = [C.red, C.orange, C.yellow, C.lime, C.green, C.sky, C.blue, C.plum];
  rect(ctx, x - 1, y - 1, cols.length * 4 + 2, 7, C.outline);
  cols.forEach((c, i) => rect(ctx, x + i * 4, y, 4, 5, c));
}

export function dashboard(ctx: Ctx, x: number, y: number, w: number, h: number, t: number) {
  rect(ctx, x - 1, y - 1, w + 2, h + 2, C.outline);
  rect(ctx, x, y, w, h, C.night);
  // Live line chart.
  for (let i = 0; i < w - 2; i++) {
    const v = Math.sin((i + t / 120) / 3) * 0.35 + Math.sin((i + t / 300) / 7) * 0.25;
    const yy = Math.round(y + h / 2 - v * (h - 3));
    px(ctx, x + 1 + i, Math.max(y + 1, Math.min(y + h - 2, yy)), C.lime);
  }
  px(ctx, x + w - 3, y + 1, Math.sin(t / 250) > 0 ? C.red : C.night);
}

export function crown(ctx: Ctx, cx: number, top: number) {
  rect(ctx, cx - 4, top + 1, 9, 4, C.outline);
  rect(ctx, cx - 3, top + 2, 7, 2, C.yellow);
  px(ctx, cx - 3, top, C.outline);
  px(ctx, cx, top, C.outline);
  px(ctx, cx + 3, top, C.outline);
  px(ctx, cx - 3, top + 1, C.yellow);
  px(ctx, cx, top + 1, C.yellow);
  px(ctx, cx + 3, top + 1, C.yellow);
  px(ctx, cx, top + 2, C.red);
}

export function noticeBoard(ctx: Ctx, x: number, y: number) {
  rect(ctx, x - 1, y - 1, 22, 12, C.outline);
  rect(ctx, x, y, 20, 10, C.wood3);
  rect(ctx, x + 2, y + 2, 6, 6, C.white);
  rect(ctx, x + 10, y + 2, 7, 4, C.yellow);
  px(ctx, x + 4, y + 2, C.red);
  px(ctx, x + 13, y + 2, C.red);
}
