/** Character sprites built from text art: auto-outlined, cached per look/frame/facing. */
import { C, px, shade } from "./draw";

/*
 * 14×20 front-facing chibi (16×22 with the outline). Keys:
 *  h hair  H hair highlight  a hair accessory  s skin  d skin shade  r blush
 *  b brow  e eye  w eye highlight  l lash  m mouth  L lips  g glasses
 *  c top  C top shade  o collar  t tie / ribbon  k skirt  p trousers  S legs (skin)  f shoes  . empty
 */
const FACE = [
  "....hhhhhh....",
  "..hhHHHhhhhh..",
  ".hhHHhhhhhhhh.",
  ".hhhhhhhhhhhh.",
  ".hhsshhhhsshh.",
  ".hsbbssssbbsh.",
  ".hsewssssewsh.",
  ".hseesssseesh.",
  ".hrssssssssrh.",
  "..ssssmmssss..",
  "...dssssssd...",
];

// Tops (rows 11–16): shirt with collar and tie / blouse with a ribbon.
const SHIRT = ["...ccottocc...", "..ccccttcccc..", ".CccccttccccC.", ".sCccccccccCs.", "..cccccccccc..", "..pppppppppp.."];
const SHIRT_TYPE = ["...ccottocc...", "..ccccttcccc..", ".CccccttccccC.", "..sccccccccs..", "..cccccccccc..", "..pppppppppp.."];
const BLOUSE = ["...ccossocc...", "..ccccttcccc..", ".CccccccccccC.", ".sCccccccccCs.", "..kkkkkkkkkk..", ".kkkkkkkkkkkk."];
const BLOUSE_TYPE = ["...ccossocc...", "..ccccttcccc..", ".CccccccccccC.", "..sccccccccs..", "..kkkkkkkkkk..", ".kkkkkkkkkkkk."];

const TROUSER_LEGS = {
  stand: ["...ppp..ppp...", "...ppp..ppp...", "...fff..fff..."],
  walkA: ["...ppp..ppp...", "...ppp..fff...", "...fff........"],
  walkB: ["...ppp..ppp...", "...fff..ppp...", "........fff..."],
};
const SKIRT_LEGS = {
  stand: ["....SS..SS....", "....SS..SS....", "...fff..fff..."],
  walkA: ["....SS..SS....", "....SS..fff...", "...fff........"],
  walkB: ["....SS..SS....", "...fff..SS....", "........fff..."],
};

export type Frame = "stand" | "walkA" | "walkB" | "type";
export type Gender = "male" | "female";

export interface Look {
  gender: Gender;
  hair: string;
  skin: string;
  shirt: string;
  /** Trousers or skirt. */
  pants: string;
  /** Hair style, per gender: male 0 side part · 1 short · 2 spiky · 3 curly; female 0 long · 1 ponytail · 2 bob · 3 bun. */
  style: number;
  /** Tie (men) or ribbon (women). */
  tie: string;
  glasses: boolean;
  skirt: boolean;
  /** Hair clip colour (women). */
  accent: string;
}

const HAIRS = ["#2b2016", "#4a3020", "#151515", "#7a4a26", "#3a2a3a", "#5a3a2a", "#8e3b2e"];
const FEMALE_HAIRS = ["#2b2016", "#4a3020", "#151515", "#7a4a26", "#9a5a36", "#5a2a3a", "#c08850"];
const SKINS = ["#f7d7bd", "#f0c3a0", "#e2ac86", "#c88d64", "#f5ceb0"];
const PANTS = ["#29366f", "#333c57", "#3d3d3d", "#566c86", "#5e3a26"];
const SKIRTS = ["#333c57", "#5d275d", "#29366f", "#3d3d3d", "#7a3b4e", "#257179"];
const ACCENTS = ["#ff8fb1", "#ffcd75", "#f4f4f4", "#73eff7", "#b13e53"];

/** Vietnamese given names (the last word of a name) that are usually female. */
const FEMALE_NAMES = new Set(
  "lan hà ha linh mai hoa hương huong thảo thao trang ngọc ngoc vy vi my mỹ nhung hằng hang yến yen thu hạnh hanh nga phương phuong quỳnh quynh trinh chi châu chau uyên uyen nhi diệp diep loan vân van huyền huyen duyên duyen hiền hien thúy thuy thủy ly lý liên lien oanh diễm diem hồng hong xuân xuan ánh ngân ngan thư trâm tram tuyết tuyet nhàn nhan nguyệt nguyet dung giao kiều kieu lệ le mơ mo nương phượng quyên quyen sương suong thơ tho tiên tien trà tra ái thắm tham"
    .split(" "),
);

export function genderOf(name: string, gender?: string): Gender {
  if (gender === "male" || gender === "female") return gender;
  const given = name.trim().split(/\s+/).pop()?.toLowerCase() ?? "";
  return FEMALE_NAMES.has(given) ? "female" : "male";
}

function hash(s: string) {
  let h = 2166136261;
  for (const c of s) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  return Math.abs(h);
}

export function lookFor(id: string, name: string, color: string, role: string, gender?: string): Look {
  const h = hash(id + "|" + name);
  const g = genderOf(name, gender);
  const female = g === "female";
  return {
    gender: g,
    hair: (female ? FEMALE_HAIRS : HAIRS)[h % HAIRS.length],
    skin: SKINS[(h >> 4) % SKINS.length],
    shirt: color,
    pants: female ? SKIRTS[(h >> 8) % SKIRTS.length] : PANTS[(h >> 8) % PANTS.length],
    style: role === "ceo" && !female ? 0 : (h >> 12) % 4,
    tie: role === "ceo" ? C.red : female ? shade(color, 0.55) : shade(color, 0.45),
    glasses: role === "ceo" || role === "architect" || (h >> 16) % 5 === 0,
    skirt: female && (h >> 20) % 5 !== 0,
    accent: ACCENTS[(h >> 22) % ACCENTS.length],
  };
}

const set = (g: string[][], r: number, c: number, k: string) => {
  if (g[r] && c >= 0 && c < g[r].length) g[r][c] = k;
};

function hairStyle(g: string[][], look: Look) {
  const female = look.gender === "female";
  if (!female) {
    // Jawline: short hair stops at the ears.
    for (const r of [7, 8]) {
      set(g, r, 1, ".");
      set(g, r, 12, ".");
    }
    set(g, 8, 2, "r");
    set(g, 8, 11, "r");
    switch (look.style) {
      case 0: // neat side part, fringe swept to one side
        g[4] = ".hhhhhhsssshh.".split("");
        break;
      case 1: // short crop, forehead visible
        g[1] = "..hhhhhhhhhh..".split("");
        g[2] = ".hhHHHhhhhhhh.".split("");
        g[4] = ".hssssssssssh.".split("");
        break;
      case 2: // spiky
        g[0] = "..h.hhhhhh.h..".split("");
        g[1] = ".hhhHHhhhhhhh.".split("");
        g[4] = ".hhshhshhshhh.".split("");
        break;
      case 3: // curly / messy
        g[0] = "...h.hhhh.h...".split("");
        g[1] = "..hHhHhhHhhh..".split("");
        g[2] = ".hhHhhhHhhhhh.".split("");
        g[4] = ".hhshhhhhshhh.".split("");
        break;
    }
    return;
  }
  // Women: lashes, lips, a hair clip and longer hair.
  set(g, 6, 2, "l");
  set(g, 6, 11, "l");
  g[9] = "..ssssLLssss..".split("");
  set(g, 2, 10, "a");
  set(g, 3, 10, "a");
  switch (look.style) {
    case 0: // long hair down past the shoulders
      for (let r = 4; r <= 10; r++) {
        set(g, r, 0, "h");
        set(g, r, 13, "h");
        set(g, r, 1, "h");
        set(g, r, 12, "h");
      }
      break;
    case 1: // ponytail on one side
      for (let r = 2; r <= 9; r++) set(g, r, 13, r === 2 ? "a" : "h");
      set(g, 3, 12, "h");
      break;
    case 2: // chin-length bob with a straight fringe
      g[4] = ".hhhhhhhhhhhh.".split("");
      for (let r = 5; r <= 9; r++) {
        set(g, r, 0, "h");
        set(g, r, 13, "h");
      }
      set(g, 10, 1, "h");
      set(g, 10, 12, "h");
      break;
    case 3: // bun on top
      g[0] = ".....hHHh.....".split("");
      g[1] = "...hhhhhhhh...".split("");
      set(g, 0, 9, "a");
      break;
  }
}

function buildRows(look: Look, frame: Frame, facing: 1 | -1): string[] {
  const head = FACE.map((r) => r.split(""));
  // Eyes follow the facing direction (and look sideways while walking).
  if (facing !== 1 || frame === "walkA" || frame === "walkB") {
    const shift = facing === 1 ? 1 : -1;
    for (const row of [6, 7]) {
      const r = head[row];
      const top = row === 6;
      r[3] = r[4] = r[9] = r[10] = "s";
      r[3 + shift] = r[9 + shift] = "e";
      r[4 + shift] = r[10 + shift] = top ? "w" : "e";
    }
  }
  hairStyle(head, look);
  if (look.glasses) {
    // Thin frames around the eyes (top rim, sides, bridge); the eyes stay visible.
    for (const col of [2, 3, 4, 5, 8, 9, 10, 11]) head[5][col] = "g";
    for (const col of [2, 5, 8, 11]) head[6][col] = head[7][col] = "g";
    head[6][6] = head[6][7] = "g";
  }
  const female = look.gender === "female";
  const typing = frame === "type";
  let body: string[];
  if (female && look.skirt) body = typing ? BLOUSE_TYPE : BLOUSE;
  else if (female) body = (typing ? BLOUSE_TYPE : BLOUSE).map((r, i) => (i >= 4 ? r.replace(/k/g, i === 4 ? "c" : "p") : r));
  else body = typing ? SHIRT_TYPE : SHIRT;
  // Long hair falls over the shoulders.
  if (female && look.style === 0) body = body.map((r, i) => (i <= 1 ? "h" + r.slice(1, 13) + "h" : r));
  const kind = female && look.skirt ? SKIRT_LEGS : TROUSER_LEGS;
  const legs = frame === "walkA" ? kind.walkA : frame === "walkB" ? kind.walkB : kind.stand;
  return [...head.map((r) => r.join("")), ...body, ...legs];
}

const cache = new Map<string, HTMLCanvasElement>();

export const SPRITE_W = 16;

/** Returns a 16×22 sprite (14×20 + 1px outline). Feet are at the bottom-centre. */
export function characterSprite(look: Look, frame: Frame, facing: 1 | -1): HTMLCanvasElement {
  const key = `${look.gender}${look.hair}${look.skin}${look.shirt}${look.pants}${look.style}${look.tie}${look.glasses}${look.skirt}${look.accent}|${frame}|${facing}`;
  const hit = cache.get(key);
  if (hit) return hit;
  const rows = buildRows(look, frame, facing);
  const W = SPRITE_W;
  const H = rows.length + 2;
  const colors: Record<string, string> = {
    h: look.hair,
    H: shade(look.hair, 0.35),
    a: look.accent,
    s: look.skin,
    S: shade(look.skin, -0.05),
    d: shade(look.skin, -0.15),
    r: look.gender === "female" ? "#f29a9a" : "#e8a08a",
    b: shade(look.hair, -0.2),
    e: "#1b1b24",
    w: "#f4f4f4",
    l: "#1b1b24",
    m: "#9a4a3a",
    L: "#d0566a",
    g: "#1b1b24",
    o: "#f4f4f4",
    c: look.shirt,
    C: shade(look.shirt, -0.25),
    t: look.tie,
    k: look.pants,
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
