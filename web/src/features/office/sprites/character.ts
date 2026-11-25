/** Character sprites built from text art: auto-outlined, cached per look/frame/facing. */
import { C, px, shade } from "./draw";

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

