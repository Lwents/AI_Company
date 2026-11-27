/** Furniture and decor painters. (x, y) is usually the bottom-left or feet point. */
import { C, rect, px, ellipse, shade, type Ctx } from "./draw";

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

export function bookshelf(ctx: Ctx, x: number, y: number, hovered = false) {
  rect(ctx, x - 1, y - 1, 32, 14, hovered ? C.yellow : C.outline);
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

/** Standing interactive library bookcase that opens the company guide. */
export function guideBookshelf(ctx: Ctx, x: number, y: number, hovered: boolean, t: number) {
  // Subtle floor shadow below the bookcase.
  rect(ctx, x, y + 18, 32, 2, "rgba(10,10,20,0.35)");
  if (hovered) {
    rect(ctx, x - 2, y - 2, 36, 22, C.yellow);
  }
  rect(ctx, x - 1, y - 1, 34, 20, hovered ? C.white : C.outline);
  rect(ctx, x, y, 32, 18, C.wood2);
  // Top crown trim + wooden feet.
  rect(ctx, x, y, 32, 1, C.wood4);
  rect(ctx, x + 1, y + 17, 4, 1, C.wood1);
  rect(ctx, x + 27, y + 17, 4, 1, C.wood1);
  const books = [C.red, C.sky, C.green, C.yellow, C.plum, C.orange, C.cyan, C.white, C.lime];
  for (let row = 0; row < 3; row++) {
    const sy = y + 2 + row * 5;
    rect(ctx, x + 2, sy, 28, 4, C.wood1);
    for (let i = 0; i < 9; i++) {
      const h = 3 + ((i + row) % 2);
      const col = row === 1 && i === 4 ? C.yellow : books[(i + row * 3) % books.length];
      rect(ctx, x + 3 + i * 3, sy + 4 - h, 2, h, col);
    }
  }
  // Soft periodic glint on the golden guide book in the middle shelf.
  if (hovered || Math.sin(t / 700) > 0.72) {
    px(ctx, x + 15, y + 7, C.white);
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
