import type { Employee, Role } from "@/api/types";

/**
 * Office floor plan. Defined in office pixels (×2 = world units used by the simulation).
 *
 *  ┌Chủ tịch┐┌─CEO─┐┌──Phòng họp──┐┌──Pantry──┐┌Marketing┐┌─Sales─┐
 *  ═══════════════════════════ hành lang ═══════════════════════════
 *  ┌─Sảnh──┐┌─Khu làm việc─┐┌Design┐║┌─QA─┐┌─CSKH─┐║┌Kế toán┐
 *  │lễ tân │|(tự mở rộng)   │└──────┘║└────┘└──────┘║└───────┘
 *  └─cửa───┘|               |┌───DevOps───┐ ┌Nhân sự & Pháp chế┐
 */

export interface P {
  x: number;
  y: number;
}

export type ZoneId =
  | "chairman"
  | "ceo"
  | "meeting"
  | "pantry"
  | "lobby"
  | "work"
  | "design"
  | "qa"
  | "devops"
  | "marketing"
  | "sales"
  | "support"
  | "finance"
  | "backoffice";

export interface Zone {
  id: ZoneId;
  label: string;
  x: number;
  y: number;
  w: number;
  h: number;
  /** Door on the wall that faces the hallway (or the corridor). */
  door: P;
  topRow: boolean;
}

export interface InteractiveBox {
  id: string;
  kind: "guide";
  label: string;
  /** Bounding box in world units. */
  x: number;
  y: number;
  w: number;
  h: number;
  /** Whether to draw a pinned label tag right below the object. */
  pinnedTag?: boolean;
}

export interface Layout {
  W: number;
  H: number;
  hallY: number;
  zones: Zone[];
  corridors: Array<{ x: number; y: number; w: number; h: number }>;
  desks: Map<string, P>;
  bookshelves: InteractiveBox[];
  chairman: P;
  receptionist: P;
  entrance: P;
  outside: P;
  meeting: { x: number; y: number; rx: number; ry: number };
  coffee: P;
}

const U = 2; // world units per office pixel
const w = (v: number) => v * U;
const pt = (x: number, y: number): P => ({ x: w(x), y: w(y) });

const ROOM_OF: Partial<Record<Role, ZoneId>> = {
  ceo: "ceo",
  tester: "qa",
  designer: "design",
  devops: "devops",
  secretary: "lobby",
  marketing: "marketing",
  sales: "sales",
  support: "support",
  accountant: "finance",
  hr: "backoffice",
  legal: "backoffice",
};

const SLOTS: Partial<Record<ZoneId, P[]>> = {
  ceo: [pt(162, 62), pt(130, 96), pt(194, 96)],
  qa: [pt(466, 196), pt(504, 196)],
  design: [pt(371, 196), pt(410, 196)],
  devops: [pt(380, 308), pt(420, 308), pt(460, 308)],
  lobby: [pt(82, 188), pt(82, 230)],
  marketing: [pt(560, 64), pt(614, 64), pt(560, 100), pt(614, 100)],
  sales: [pt(676, 64), pt(732, 64), pt(676, 100), pt(732, 100)],
  support: [pt(554, 196), pt(616, 196)],
  finance: [pt(680, 196), pt(736, 196)],
  backoffice: [pt(574, 308), pt(706, 308), pt(744, 308)],
};

const WORK_X = 112;
const WORK_COLS = [150, 225, 300];
const WORK_ROW0 = 184;
const WORK_ROW_H = 46;

export function buildLayout(staff: Employee[]): Layout {
  // Assign desks: own room when it has space, otherwise the open work area.
  const desks = new Map<string, P>();
  const used: Partial<Record<ZoneId, number>> = {};
  const toWork: Employee[] = [];
  for (const e of staff) {
    const room = ROOM_OF[e.role];
    const slots = room ? SLOTS[room] : undefined;
    const n = room ? (used[room] ?? 0) : 0;
    if (room && slots && n < slots.length) {
      desks.set(e.id, slots[n]);
      used[room] = n + 1;
    } else {
      toWork.push(e);
    }
  }
  const rows = Math.max(4, Math.ceil(toWork.length / WORK_COLS.length));
  toWork.forEach((e, i) => {
    desks.set(e.id, pt(WORK_COLS[i % WORK_COLS.length], WORK_ROW0 + Math.floor(i / WORK_COLS.length) * WORK_ROW_H));
  });
  const workH = Math.max(196, WORK_ROW0 - 148 + (rows - 1) * WORK_ROW_H + 26);
  const bottom = Math.max(344, 148 + workH);

  const zones: Zone[] = [
    { id: "chairman", label: "Phòng Chủ tịch", x: w(6), y: w(6), w: w(100), h: w(110), door: pt(80, 116), topRow: true },
    { id: "ceo", label: "Phòng CEO", x: w(112), y: w(6), w: w(100), h: w(110), door: pt(186, 116), topRow: true },
    { id: "meeting", label: "Phòng họp", x: w(218), y: w(6), w: w(150), h: w(110), door: pt(293, 116), topRow: true },
    { id: "pantry", label: "Pantry", x: w(374), y: w(6), w: w(150), h: w(110), door: pt(400, 116), topRow: true },
    { id: "lobby", label: "Sảnh & Nhân sự", x: w(6), y: w(148), w: w(100), h: w(110), door: pt(56, 148), topRow: false },
    { id: "work", label: "Khu làm việc", x: w(WORK_X), y: w(148), w: w(232), h: w(workH), door: pt(226, 148), topRow: false },
    { id: "design", label: "Design", x: w(350), y: w(148), w: w(80), h: w(96), door: pt(390, 148), topRow: false },
    { id: "qa", label: "Phòng QA", x: w(446), y: w(148), w: w(78), h: w(96), door: pt(485, 148), topRow: false },
    { id: "devops", label: "DevOps & Server", x: w(350), y: w(262), w: w(174), h: w(82), door: pt(438, 262), topRow: false },
    { id: "marketing", label: "Marketing", x: w(532), y: w(6), w: w(110), h: w(110), door: pt(587, 116), topRow: true },
    { id: "sales", label: "Sales", x: w(648), y: w(6), w: w(112), h: w(110), door: pt(704, 116), topRow: true },
    { id: "support", label: "CSKH", x: w(532), y: w(148), w: w(108), h: w(96), door: pt(586, 148), topRow: false },
    { id: "finance", label: "Kế toán", x: w(656), y: w(148), w: w(104), h: w(96), door: pt(708, 148), topRow: false },
    { id: "backoffice", label: "Nhân sự & Pháp chế", x: w(532), y: w(262), w: w(228), h: w(82), door: pt(648, 262), topRow: false },
  ];

  return {
    W: w(766),
    H: w(bottom + 14),
    hallY: w(132),
    zones,
    corridors: [
      { x: w(430), y: w(148), w: w(16), h: w(114) },
      { x: w(640), y: w(148), w: w(16), h: w(114) },
    ],
    desks,
    bookshelves: [
      {
        id: "chairman-bookshelf",
        kind: "guide",
        label: "Tủ sách",
        x: w(68),
        y: w(7),
        w: w(32),
        h: w(18),
        pinnedTag: true,
      },
      {
        id: "ceo-bookshelf",
        kind: "guide",
        label: "Tủ sách",
        x: w(176),
        y: w(7),
        w: w(30),
        h: w(12),
      },
    ],
    chairman: pt(56, 62),
    receptionist: pt(30, 188),
    entrance: pt(56, 246),
    outside: pt(56, 270),
    meeting: { x: w(293), y: w(68), rx: w(50), ry: w(17) },
    coffee: pt(452, 46),
  };
}

export function zoneAt(l: Layout, p: P): Zone | null {
  return l.zones.find((z) => p.x >= z.x && p.x <= z.x + z.w && p.y >= z.y && p.y <= z.y + z.h) ?? null;
}

export function meetingSeats(l: Layout, n: number): P[] {
  const seats: P[] = [];
  const { x, y, rx, ry } = l.meeting;
  for (let i = 0; i < n; i++) {
    const a = -Math.PI / 2 + (i / Math.max(n, 1)) * Math.PI * 2;
    seats.push({ x: x + Math.cos(a) * (rx + 22), y: y + Math.sin(a) * (ry + 24) + 10 });
  }
  return seats;
}

/** Where a visitor stands when bringing something to this desk. */
export function visitPoint(desk: P): P {
  return { x: desk.x + 32, y: desk.y + 8 };
}

/** Route through doors and the hallway so people never walk through walls. */
export function route(l: Layout, from: P, to: P): P[] {
  const a = zoneAt(l, from);
  const b = zoneAt(l, to);
  if (a && b && a.id === b.id) return [to];
  const pts: P[] = [];
  const inside = (z: Zone) => ({ x: z.door.x, y: z.door.y + (z.topRow ? -14 : 14) });
  if (a) pts.push(inside(a), { x: a.door.x, y: l.hallY });
  else pts.push({ x: from.x, y: l.hallY });
  if (b) pts.push({ x: b.door.x, y: l.hallY }, inside(b));
  else pts.push({ x: to.x, y: l.hallY });
  pts.push(to);
  return pts;
}
