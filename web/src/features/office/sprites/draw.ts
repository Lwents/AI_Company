/**
 * Pixel drawing primitives and the palette. All coordinates are in *pixel units*
 * of the low-resolution office canvas (world / 2).
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
