import type { CompanyState } from "@/api/types";
import type { Layout, Zone, ZoneId } from "../layout";
import { p, type Scene } from "../engine/scene";
import { C, ellipse, px, rect, shade, type Ctx } from "../sprites/draw";
import {
  bookshelf,
  coffeeCorner,
  dashboard,
  easel,
  fridge,
  globe,
  guideBookshelf,
  noticeBoard,
  painting,
  plant,
  printer,
  serverRack,
  sofa,
  swatches,
  waterCooler,
  windowPane,
} from "../sprites/furniture";

/** Everything a room painter needs. Coordinates are office pixels. */
export interface RoomEnv {
  ctx: Ctx;
  l: Layout;
  z: Zone;
  x: number;
  y: number;
  w: number;
  h: number;
  doorX: number;
  now: number;
  st: CompanyState;
  scene: Scene;
}

type FloorPainter = (ctx: Ctx, x0: number, y0: number, w: number, h: number) => void;
type DecorPainter = (e: RoomEnv) => void;

/* ------------------------------------------------------------------ floors */

/** Floor pattern per room. To add a room: add its zone in layout.ts, then a floor and a decor entry here. */
const FLOORS: Record<ZoneId, FloorPainter> = {
  chairman(ctx, x0, y0, w, h) {
    rect(ctx, x0, y0, w, h, "#5b2a3a");
    for (let y = y0; y < y0 + h; y += 6) for (let x = x0 + ((y / 6) % 2) * 3; x < x0 + w; x += 6) px(ctx, x, y, "#6b3445");
  },
  ceo(ctx, x0, y0, w, h) {
    for (let y = y0; y < y0 + h; y += 5) {
      rect(ctx, x0, y, w, 5, (y / 5) % 2 ? C.wood3 : shade(C.wood3, -0.06));
      rect(ctx, x0, y + 4, w, 1, C.wood2);
      const off = ((y / 5) % 3) * 13;
      for (let x = x0 + off; x < x0 + w; x += 38) rect(ctx, x, y, 1, 4, C.wood2);
    }
  },
  meeting(ctx, x0, y0, w, h) {
    rect(ctx, x0, y0, w, h, "#3c5a8a");
    for (let y = y0; y < y0 + h; y += 4) for (let x = x0 + ((y / 4) % 2) * 2; x < x0 + w; x += 4) px(ctx, x, y, "#46679a");
  },
  pantry(ctx, x0, y0, w, h) {
    for (let y = y0; y < y0 + h; y += 8) for (let x = x0; x < x0 + w; x += 8) rect(ctx, x, y, 8, 8, ((x - x0) / 8 + (y - y0) / 8) % 2 ? C.white : "#c9d6df");
  },
  lobby(ctx, x0, y0, w, h) {
    for (let y = y0; y < y0 + h; y += 10)
      for (let x = x0; x < x0 + w; x += 10) {
        rect(ctx, x, y, 10, 10, ((x - x0) / 10 + (y - y0) / 10) % 2 ? "#d8c8a8" : "#cdbb98");
        px(ctx, x, y, "#b9a680");
      }
  },
  work(ctx, x0, y0, w, h) {
    for (let y = y0; y < y0 + h; y += 12)
      for (let x = x0; x < x0 + w; x += 12) {
        rect(ctx, x, y, 12, 12, ((x - x0) / 12 + (y - y0) / 12) % 2 ? "#4b5a7a" : "#475574");
        px(ctx, x + 3, y + 4, "#56678a");
        px(ctx, x + 8, y + 9, "#56678a");
      }
  },
  design(ctx, x0, y0, w, h) {
    rect(ctx, x0, y0, w, h, "#e9dcc9");
    for (let y = y0; y < y0 + h; y += 6) rect(ctx, x0, y, w, 1, "#dccdb6");
  },
  qa(ctx, x0, y0, w, h) {
    for (let y = y0; y < y0 + h; y += 10)
      for (let x = x0; x < x0 + w; x += 10) {
        rect(ctx, x, y, 10, 10, "#3f6b66");
        rect(ctx, x, y, 10, 1, "#36605b");
        rect(ctx, x, y, 1, 10, "#36605b");
      }
  },
  devops(ctx, x0, y0, w, h) {
    for (let y = y0; y < y0 + h; y += 8)
      for (let x = x0; x < x0 + w; x += 8) {
        rect(ctx, x, y, 8, 8, "#2a2f45");
        rect(ctx, x, y, 8, 1, "#232739");
        px(ctx, x + 4, y + 4, "#363c58");
      }
  },
  marketing(ctx, x0, y0, w, h) {
    for (let y = y0; y < y0 + h; y += 8)
      for (let x = x0; x < x0 + w; x += 8) rect(ctx, x, y, 8, 8, ((x - x0) / 8 + (y - y0) / 8) % 2 ? "#f2a477" : "#ec9465");
  },
  sales(ctx, x0, y0, w, h) {
    rect(ctx, x0, y0, w, h, "#2f5a46");
    for (let y = y0; y < y0 + h; y += 4) for (let x = x0 + ((y / 4) % 2) * 2; x < x0 + w; x += 4) px(ctx, x, y, "#386a53");
  },
  support(ctx, x0, y0, w, h) {
    for (let y = y0; y < y0 + h; y += 10)
      for (let x = x0; x < x0 + w; x += 10) {
        rect(ctx, x, y, 10, 10, "#4a4f7a");
        px(ctx, x + 5, y + 5, "#5a6090");
      }
  },
  finance(ctx, x0, y0, w, h) {
    for (let y = y0; y < y0 + h; y += 5) {
      rect(ctx, x0, y, w, 5, (y / 5) % 2 ? "#7c6a4e" : "#756347");
      rect(ctx, x0, y + 4, w, 1, "#5e4f38");
    }
  },
  backoffice(ctx, x0, y0, w, h) {
    rect(ctx, x0, y0, w, h, "#55607e");
    for (let y = y0; y < y0 + h; y += 6) for (let x = x0 + ((y / 6) % 2) * 3; x < x0 + w; x += 6) px(ctx, x, y, "#5e6a8a");
  },
};

/* ------------------------------------------------------------------ decor */

const DECOR: Record<ZoneId, DecorPainter> = {
  chairman({ ctx, x, y, h, doorX, now, scene }) {
    windowPane(ctx, x + 6, y + 3, 20, 8, now);
    painting(ctx, x + 34, y + 2, 16, 10);
    guideBookshelf(ctx, x + 62, y + 1, scene.hoveredBookshelf === "chairman-bookshelf", now);
    rect(ctx, x + 14, y + 26, 72, 40, C.plum);
    rect(ctx, x + 16, y + 28, 68, 36, shade(C.plum, 0.12));
    rect(ctx, doorX - 6, y + 70, 12, h - 73, C.red);
    rect(ctx, doorX - 6, y + 70, 12, 1, C.yellow);
    globe(ctx, x + 88, y + 42, now);
    plant(ctx, x + 6, y + h - 6);
    // Trophy cabinet.
    rect(ctx, x + 6, y + 70, 10, 20, C.outline);
    rect(ctx, x + 7, y + 71, 8, 18, C.wood2);
    rect(ctx, x + 9, y + 74, 4, 4, C.yellow);
    rect(ctx, x + 9, y + 82, 4, 4, C.silver);
  },
  ceo({ ctx, x, y, w, h, now, scene }) {
    windowPane(ctx, x + 6, y + 3, 22, 8, now);
    windowPane(ctx, x + 34, y + 3, 22, 8, now);
    bookshelf(ctx, x + 64, y + 1, scene.hoveredBookshelf === "ceo-bookshelf");
    // Rug.
    rect(ctx, x + 16, y + 30, 68, 44, C.plum);
    rect(ctx, x + 18, y + 32, 64, 40, C.red);
    rect(ctx, x + 20, y + 34, 60, 36, C.plum);
    plant(ctx, x + 6, y + 50);
    plant(ctx, x + w - 8, y + h - 6);
  },
  meeting({ ctx, l, x, y, w, now, scene }) {
    windowPane(ctx, x + 8, y + 3, 26, 8, now);
    windowPane(ctx, x + 40, y + 3, 26, 8, now);
    // Whiteboard (the topic text is drawn crisp in the overlay).
    rect(ctx, x + 98, y + 1, 48, 12, C.silver);
    rect(ctx, x + 99, y + 2, 46, 10, C.white);
    if (!scene.meeting) {
      rect(ctx, x + 102, y + 5, 14, 1, C.sky);
      rect(ctx, x + 102, y + 8, 22, 1, C.red);
    }
    // Oval table with a conference phone.
    const mt = l.meeting;
    ellipse(ctx, p(mt.x), p(mt.y) + 2, p(mt.rx) + 1, p(mt.ry) + 1, C.outline);
    ellipse(ctx, p(mt.x), p(mt.y) + 2, p(mt.rx), p(mt.ry), C.wood1);
    ellipse(ctx, p(mt.x), p(mt.y), p(mt.rx), p(mt.ry), C.wood3);
    ellipse(ctx, p(mt.x) - 8, p(mt.y) - 3, p(mt.rx) - 18, p(mt.ry) - 6, C.wood4);
    rect(ctx, p(mt.x) - 6, p(mt.y) - 3, 12, 6, C.ink);
    rect(ctx, p(mt.x) - 5, p(mt.y) - 2, 10, 4, scene.meeting ? C.cyan : C.slate);
    rect(ctx, p(mt.x) + 16, p(mt.y) + 1, 6, 4, C.white);
    plant(ctx, x + w - 8, y + 30);
  },
  pantry({ ctx, l, x, y, w, h, now }) {
    windowPane(ctx, x + 8, y + 3, 30, 8, now);
    // Wall clock.
    rect(ctx, x + 46, y + 2, 10, 10, C.outline);
    rect(ctx, x + 47, y + 3, 8, 8, C.white);
    const ang = now / 3000;
    px(ctx, x + 51 + Math.round(Math.cos(ang) * 2), y + 7 + Math.round(Math.sin(ang) * 2), C.red);
    px(ctx, x + 51, y + 7, C.outline);
    coffeeCorner(ctx, p(l.coffee.x) - 35, y + 30, now);
    fridge(ctx, x + w - 16, y + 4);
    sofa(ctx, x + 6, y + h - 22, 44);
    ellipse(ctx, x + 72, y + h - 16, 9, 4, C.outline);
    ellipse(ctx, x + 72, y + h - 17, 8, 3, C.white);
    plant(ctx, x + 6, y + 56);
  },
  lobby({ ctx, x, y, w, h, doorX, now }) {
    // "AI" company sign.
    rect(ctx, x + 36, y + 2, 28, 10, C.orange);
    rect(ctx, x + 37, y + 3, 26, 8, shade(C.orange, 0.2));
    rect(ctx, x + 41, y + 5, 2, 4, C.outline);
    rect(ctx, x + 45, y + 5, 2, 4, C.outline);
    rect(ctx, x + 42, y + 5, 4, 1, C.outline);
    rect(ctx, x + 42, y + 7, 4, 1, C.outline);
    rect(ctx, x + 50, y + 5, 2, 4, C.outline);
    noticeBoard(ctx, x + 72, y + 2);
    windowPane(ctx, x + 6, y + 3, 22, 8, now);
    sofa(ctx, x + 6, y + h - 42, 26);
    plant(ctx, x + w - 8, y + h - 6);
    // Front door.
    rect(ctx, doorX - 10, y + h - 4, 20, 4, C.wood2);
    rect(ctx, doorX - 10, y + h, 20, 2, C.cyan);
  },
  work({ ctx, x, y, w, h, now, st, scene }) {
    for (let i = 0; i < 2; i++) windowPane(ctx, x + 8 + i * 30, y + 3, 22, 8, now);
    kanbanBoard(ctx, x + 130, y + 1, st, scene.selectedProject);
    plant(ctx, x + 6, y + h - 4);
    plant(ctx, x + w - 8, y + h - 4);
    printer(ctx, x + w - 24, y + 20);
  },
  design({ ctx, x, y, w, h }) {
    swatches(ctx, x + 6, y + 4);
    painting(ctx, x + 46, y + 2, 14, 10);
    easel(ctx, x + w - 16, y + 62);
    plant(ctx, x + 8, y + h - 4);
  },
  qa({ ctx, x, y, w, h, now }) {
    // Bug board.
    rect(ctx, x + 6, y + 2, 30, 10, C.yellow);
    for (let i = 0; i < 4; i++) rect(ctx, x + 9 + i * 7, y + 4 + (i % 2) * 3, 4, 3, i % 2 ? C.red : C.green);
    windowPane(ctx, x + 44, y + 3, 22, 8, now);
    plant(ctx, x + w - 8, y + h - 4);
  },
  marketing({ ctx, x, y, w, h, now }) {
    windowPane(ctx, x + 6, y + 3, 20, 8, now);
    // Campaign posters.
    rect(ctx, x + 32, y + 2, 12, 11, C.outline);
    rect(ctx, x + 33, y + 3, 10, 9, C.yellow);
    rect(ctx, x + 35, y + 5, 6, 2, C.red);
    rect(ctx, x + 35, y + 8, 4, 1, C.plum);
    rect(ctx, x + 48, y + 2, 12, 11, C.outline);
    rect(ctx, x + 49, y + 3, 10, 9, C.sky);
    rect(ctx, x + 51, y + 5, 6, 5, C.white);
    // Rising chart.
    rect(ctx, x + 66, y + 2, 36, 11, C.outline);
    rect(ctx, x + 67, y + 3, 34, 9, C.white);
    for (let i = 0; i < 6; i++) {
      const wave = Math.round(Math.sin(now / 600 + i) + 1);
      rect(ctx, x + 69 + i * 5, y + 11 - (i + 1) - wave, 3, i + 1 + wave, i % 2 ? C.orange : C.red);
    }
    plant(ctx, x + w - 8, y + h - 6);
  },
  sales({ ctx, x, y, w, h, now, st }) {
    windowPane(ctx, x + 6, y + 3, 20, 8, now);
    // Sales target board: one green block per closed deal.
    rect(ctx, x + 32, y + 2, 40, 11, C.outline);
    rect(ctx, x + 33, y + 3, 38, 9, C.white);
    const deals = (st.deals ?? []).length;
    for (let i = 0; i < 8; i++) rect(ctx, x + 35 + i * 4, y + 5, 3, 5, i < deals ? C.green : "#d6dde6");
    // Deal bell.
    rect(ctx, x + 82, y + 4, 8, 7, C.yellow);
    rect(ctx, x + 85, y + 2, 2, 2, C.wood1);
    px(ctx, x + 86, y + 11, C.wood1);
    plant(ctx, x + w - 8, y + h - 6);
  },
  support({ ctx, x, y, w, h, now, st }) {
    // Live queue display: one block per open case.
    rect(ctx, x + 6, y + 2, 44, 11, C.outline);
    rect(ctx, x + 7, y + 3, 42, 9, C.night);
    const waiting = (st.cases ?? []).filter((c) => c.status === "open" || c.status === "in_progress").length;
    for (let i = 0; i < Math.min(waiting, 8); i++) rect(ctx, x + 9 + i * 5, y + 5, 3, 5, Math.sin(now / 200 + i) > 0 ? C.red : C.orange);
    if (waiting === 0) rect(ctx, x + 9, y + 7, 38, 1, C.green);
    windowPane(ctx, x + 58, y + 3, 22, 8, now);
    plant(ctx, x + w - 8, y + h - 4);
  },
  finance({ ctx, x, y, w, h, now }) {
    windowPane(ctx, x + 6, y + 3, 22, 8, now);
    // Safe.
    rect(ctx, x + w - 22, y + 18, 16, 18, C.outline);
    rect(ctx, x + w - 21, y + 19, 14, 16, C.slate);
    ellipse(ctx, x + w - 14, y + 27, 3, 3, C.silver);
    px(ctx, x + w - 14, y + 27, C.outline);
    // Money chart on the wall.
    rect(ctx, x + 34, y + 2, 30, 11, C.outline);
    rect(ctx, x + 35, y + 3, 28, 9, "#e9f7e9");
    for (let i = 0; i < 7; i++) px(ctx, x + 37 + i * 4, y + 10 - Math.min(6, i + (Math.floor(now / 900) % 2)), C.green);
    plant(ctx, x + 8, y + h - 4);
  },
  backoffice({ ctx, x, y, w, h, now }) {
    // File cabinets.
    for (let i = 0; i < 3; i++) {
      rect(ctx, x + 4 + i * 11, y + 18, 10, 22, C.outline);
      rect(ctx, x + 5 + i * 11, y + 19, 8, 20, C.silver);
      rect(ctx, x + 7 + i * 11, y + 23, 4, 1, C.slate);
      rect(ctx, x + 7 + i * 11, y + 31, 4, 1, C.slate);
    }
    // Scales of justice (legal).
    rect(ctx, x + w - 20, y + 3, 1, 9, C.yellow);
    rect(ctx, x + w - 26, y + 4, 13, 1, C.yellow);
    rect(ctx, x + w - 27, y + 6, 4, 2, C.yellow);
    rect(ctx, x + w - 16, y + 6, 4, 2, C.yellow);
    noticeBoard(ctx, x + 50, y + 2);
    windowPane(ctx, x + 80, y + 3, 22, 8, now);
    plant(ctx, x + w - 8, y + h - 4);
  },
  devops({ ctx, x, y, w, h, now }) {
    dashboard(ctx, x + 6, y + 2, 36, 10, now);
    dashboard(ctx, x + 48, y + 2, 24, 10, now + 900);
    for (let i = 0; i < 3; i++) serverRack(ctx, x + w - 16 - i * 15, y + 20, now + i * 333);
    // Cable tray with a data blip running along it.
    rect(ctx, x + 2, y + h - 6, w - 4, 2, C.ink);
    px(ctx, x + 20 + ((now / 40) % (w - 40)), y + h - 6, C.lime);
  },
};

/* ------------------------------------------------------------------ painters */

/** Hallway between the two rows, the side corridors, and the front-door light. */
export function drawHallway(ctx: Ctx, l: Layout, now: number) {
  const hy = p(l.hallY);
  const PW = p(l.W);
  const top = hy - 16;
  for (let x = 6; x < PW - 6; x += 10)
    for (let y = top; y < top + 32; y += 8) rect(ctx, x, y, 10, 8, ((x - 6) / 10 + (y - top) / 8) % 2 ? "#3f4a6e" : "#3a4466");
  // Red carpet.
  rect(ctx, 6, hy - 3, PW - 12, 6, C.red);
  rect(ctx, 6, hy - 3, PW - 12, 1, shade(C.red, 0.2));
  rect(ctx, 6, hy + 2, PW - 12, 1, shade(C.red, -0.3));
  for (const c of l.corridors) {
    for (let y = p(c.y); y < p(c.y + c.h); y += 8)
      for (let x = p(c.x); x < p(c.x + c.w); x += 8) rect(ctx, x, y, 8, 8, ((x / 8 + y / 8) | 0) % 2 ? "#3f4a6e" : "#3a4466");
    rect(ctx, p(c.x) + 6, p(c.y), 4, p(c.h), shade(C.red, -0.1));
  }
  waterCooler(ctx, 360, hy - 15);
  plant(ctx, 14, hy + 14);
  plant(ctx, PW - 12, hy + 14);
  const blink = Math.sin(now / 500) > 0;
  px(ctx, p(l.outside.x), p(l.outside.y) + 2, blink ? C.lime : C.green);
}

/** Floor, walls, doorway and decor of one room. */
export function drawRoom(ctx: Ctx, l: Layout, z: Zone, now: number, st: CompanyState, scene: Scene) {
  const x = p(z.x);
  const y = p(z.y);
  const w = p(z.w);
  const h = p(z.h);
  const BAND = 14;
  const doorX = p(z.door.x);
  const wallFace = z.id === "chairman" ? "#d9c7a0" : z.id === "devops" ? "#8a96ad" : z.topRow ? "#c2c8d6" : "#b8c0d0";
  const cap = C.ink;
  const floor = FLOORS[z.id];

  ctx.save();
  ctx.beginPath();
  ctx.rect(x, y, w, h);
  ctx.clip();
  floor(ctx, x, y, w, h);
  ctx.restore();
  rect(ctx, x, y, w, BAND, wallFace);
  rect(ctx, x, y, w, 2, cap);
  rect(ctx, x, y + BAND - 2, w, 2, shade(wallFace, -0.25));
  if (!z.topRow) {
    // Bottom-row rooms are entered through their back wall: cut a doorway.
    ctx.save();
    ctx.beginPath();
    ctx.rect(doorX - 10, y, 20, BAND);
    ctx.clip();
    floor(ctx, x, y, w, h);
    ctx.restore();
    rect(ctx, doorX - 11, y, 1, BAND, cap);
    rect(ctx, doorX + 10, y, 1, BAND, cap);
    rect(ctx, x - 2, y + h, w + 4, 2, cap);
  } else {
    rect(ctx, x, y + h - 3, doorX - 10 - x, 3, cap);
    rect(ctx, doorX + 10, y + h - 3, x + w - doorX - 10, 3, cap);
  }
  rect(ctx, x - 2, y, 2, h, cap);
  rect(ctx, x + w, y, 2, h, cap);
  // Doormat.
  rect(ctx, doorX - 8, z.topRow ? y + h - 7 : y + BAND, 16, 4, C.wood2);

  DECOR[z.id]({ ctx, l, z, x, y, w, h, doorX, now, st, scene });
}

/** Tiny kanban board on the work-area wall: ticket counts per column for the selected project. */
function kanbanBoard(ctx: Ctx, x: number, y: number, st: CompanyState, projectId: string | null) {
  const tickets = st.tickets.filter((t) => t.projectId === projectId);
  rect(ctx, x - 1, y - 1, 74, 14, C.outline);
  rect(ctx, x, y, 72, 12, C.wood4);
  rect(ctx, x + 1, y + 1, 70, 10, "#e8d9b8");
  const cols: Array<[string, string[]]> = [
    [C.silver, ["todo"]],
    [C.sky, ["in_progress", "review"]],
    [C.yellow, ["testing"]],
    [C.green, ["done"]],
  ];
  cols.forEach(([color, sts], i) => {
    const cx = x + 2 + i * 18;
    rect(ctx, cx, y + 2, 16, 1, shade(color, -0.3));
    const n = tickets.filter((t) => sts.includes(t.status)).length;
    for (let k = 0; k < Math.min(n, 6); k++) rect(ctx, cx + (k % 3) * 5, y + 4 + Math.floor(k / 3) * 4, 4, 3, color);
    if (i < 3) rect(ctx, cx + 16, y + 2, 1, 8, "#c8b894");
  });
}
