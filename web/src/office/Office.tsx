import { useEffect, useRef } from "react";
import { useStore } from "../store";
import { ROLE_LABEL } from "../labels";
import { topPerformer } from "../kpi";
import type { CompanyState, EmpStatus, Employee } from "../types";
import { buildLayout, meetingSeats, route, visitPoint, type Layout, type P, type Zone } from "./layout";
import {
  C,
  bookshelf,
  chair,
  characterSprite,
  coffeeCorner,
  crown,
  dashboard,
  desk,
  easel,
  ellipse,
  fridge,
  globe,
  lookFor,
  noticeBoard,
  painting,
  plant,
  printer,
  px,
  receptionCounter,
  rect,
  serverRack,
  shade,
  sofa,
  swatches,
  waterCooler,
  windowPane,
  type Ctx,
  type Frame,
  type Look,
} from "./pixel";

/** World units per office pixel. */
const U = 2;
const FONT = "VT323, ui-monospace, monospace";
const SPEED = 110; // world units per second
/** Seated people are drawn this many office pixels higher, so the upper body shows above the desk. */
const SEAT = 5;
const p = (v: number) => Math.round(v / U);
/** Scratch canvas for sharp-bilinear scaling at non-integer zoom. */
let bigCanvas: HTMLCanvasElement | null = null;

type Task =
  | { kind: "deliver"; to: string; text: string; stage: "go" | "talk" | "back"; until?: number }
  | { kind: "coffee"; stage: "go" | "sip" | "back"; until?: number };

interface Actor {
  id: string;
  emp: Employee;
  x: number;
  y: number;
  path: P[];
  queue: Task[];
  task?: Task;
  carrying: boolean;
  bubble?: { text: string; until: number };
  facing: 1 | -1;
  walk: number;
  moving: boolean;
  /** Walking out of the building (fired / on leave). */
  leaving: boolean;
}

const CHAIRMAN_LOOK: Look = { hair: "#2b2016", skin: "#f2c8a0", shirt: "#ffcd75", pants: "#29366f", style: 0, tie: C.red, glasses: true };
const RECEPTION_LOOK: Look = { hair: "#7a4a26", skin: "#f5d0b0", shirt: "#73eff7", pants: "#333c57", style: 1, tie: "#f4f4f4", glasses: false };

export function Office() {
  const { state, onLive, selectedEmployee, setSelectedEmployee, selectedProject } = useStore();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const stateRef = useRef<CompanyState | null>(state);
  const selRef = useRef<{ emp: string | null; project: string | null }>({ emp: null, project: null });
  const actors = useRef(new Map<string, Actor>());
  const initialized = useRef(false);
  const meeting = useRef<{ participants: string[]; topic: string } | null>(null);
  const mouse = useRef<P | null>(null);
  const crownRef = useRef<string | null>(null);
  /** World → device pixels: screen = o + world * scale.  Camera: fit, or manual zoom + pan. */
  const view = useRef({ scale: 1, ox: 0, oy: 0, k: 1 });
  const cam = useRef<{ manual: boolean; zoom: number; cx: number; cy: number }>({ manual: false, zoom: 2, cx: 0, cy: 0 });
  const drag = useRef<{ x: number; y: number; cx: number; cy: number; moved: boolean } | null>(null);

  stateRef.current = state;
  selRef.current = { emp: selectedEmployee, project: selectedProject };

  useEffect(() => {
    const off = onLive((e) => {
      if (e.type === "meeting") {
        meeting.current = e.active ? { participants: e.participants, topic: e.topic } : null;
        return;
      }
      if (e.from === "customer") {
        const t = actors.current.get(e.to);
        if (t) t.bubble = { text: e.text || "☎", until: performance.now() + 3500 };
        return;
      }
      const a = actors.current.get(e.from);
      if (!a || a.leaving) return;
      if (a.task?.kind === "coffee") {
        a.task = undefined;
        a.path = [];
      }
        if (a.queue.length < 4) a.queue.push({ kind: "deliver", to: e.to, text: e.text, stage: "go" });
    });
    return () => {
      off();
    };
  }, [onLive]);

  useEffect(() => {
    const canvas = canvasRef.current!;
    const ctx = canvas.getContext("2d")!;
    const off = document.createElement("canvas");
    const octx = off.getContext("2d")!;
    let raf = 0;
    let last = performance.now();
    let sizeKey = "";
    let crownAt = 0;
    void document.fonts?.load(`16px VT323`);

    const frame = (now: number) => {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      const st = stateRef.current;
      if (st) {
        const layout = buildLayout(st.employees.filter((e) => e.enabled));
        if (now - crownAt > 1000) {
          crownRef.current = topPerformer(st);
          crownAt = now;
        }
        const key = `${layout.W}x${layout.H}`;
        if (key !== sizeKey) {
          off.width = p(layout.W);
          off.height = p(layout.H);
          sizeKey = key;
        }
        fitView(canvas, layout);
        tick(st, layout, dt, now);
        drawWorld(octx, st, layout, now);
        compose(ctx, off, st, layout, now);
      }
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, []);

  // ---------------- camera ----------------

  function fitView(canvas: HTMLCanvasElement, l: Layout) {
    const wrap = wrapRef.current!;
    const dpr = window.devicePixelRatio || 1;
    const cw = Math.round(wrap.clientWidth * dpr);
    const ch = Math.round(wrap.clientHeight * dpr);
    if (canvas.width !== cw || canvas.height !== ch) {
      canvas.width = cw;
      canvas.height = ch;
      canvas.style.width = `${wrap.clientWidth}px`;
      canvas.style.height = `${wrap.clientHeight}px`;
    }
    const PW = p(l.W);
    const PH = p(l.H);
    const c = cam.current;
    if (!c.manual) {
      const fit = Math.min(cw / PW, ch / PH);
      const k = fit >= 2 ? Math.floor(fit) : fit;
      view.current = { k, scale: k / U, ox: Math.floor((cw - PW * k) / 2), oy: Math.floor((ch - PH * k) / 2) };
      return;
    }
    const k = c.zoom;
    const halfW = cw / k / 2;
    const halfH = ch / k / 2;
    c.cx = PW * k <= cw ? PW / 2 : Math.min(PW - halfW, Math.max(halfW, c.cx));
    c.cy = PH * k <= ch ? PH / 2 : Math.min(PH - halfH, Math.max(halfH, c.cy));
    view.current = { k, scale: k / U, ox: Math.round(cw / 2 - c.cx * k), oy: Math.round(ch / 2 - c.cy * k) };
  }

  // ---------------- simulation (world units) ----------------

  function homeOf(st: CompanyState, l: Layout, id: string): P {
    const m = meeting.current;
    if (m) {
      const present = m.participants.filter((x) => st.employees.some((e) => e.id === x && e.enabled));
      const i = present.indexOf(id);
      if (i >= 0) return meetingSeats(l, present.length)[i];
    }
    return l.desks.get(id) ?? { x: l.entrance.x, y: l.hallY };
  }

  function tick(st: CompanyState, l: Layout, dt: number, now: number) {
    const staff = st.employees.filter((e) => e.enabled);
    for (const e of staff) {
      const a = actors.current.get(e.id);
      if (a) {
        a.emp = e;
        if (a.leaving) {
          a.leaving = false;
          a.path = [];
        }
        continue;
      }
      const d = l.desks.get(e.id)!;
      // On first load everyone is already at their desk; new hires walk in through the front door.
      const fresh = initialized.current;
      const start = fresh ? l.outside : d;
      actors.current.set(e.id, {
        id: e.id,
        emp: e,
        x: start.x,
        y: start.y,
        path: fresh ? [l.entrance] : [],
        queue: [],
        carrying: false,
        facing: 1,
        walk: 0,
        moving: false,
        leaving: false,
        bubble: fresh ? { text: "Xin chào!", until: now + 4000 } : undefined,
      });
    }
    initialized.current = true;
    for (const a of actors.current.values()) {
      if (!a.leaving && !staff.some((e) => e.id === a.id)) {
        a.leaving = true;
        a.task = undefined;
        a.queue = [];
        a.carrying = false;
        a.bubble = { text: "Tạm biệt!", until: now + 3000 };
        a.path = [...route(l, a, l.entrance), l.outside];
      }
    }

    for (const a of actors.current.values()) {
      const rt = st.runtime[a.id];
      const busy = rt && (rt.status === "thinking" || rt.status === "working" || rt.status === "waiting");
      const home = homeOf(st, l, a.id);

      a.moving = false;
      if (a.path.length) {
        const t = a.path[0];
        const dx = t.x - a.x;
        const dy = t.y - a.y;
        const d = Math.hypot(dx, dy);
        const step = SPEED * dt;
        if (d <= step) {
          a.x = t.x;
          a.y = t.y;
          a.path.shift();
        } else {
          a.x += (dx / d) * step;
          a.y += (dy / d) * step;
          if (Math.abs(dx) > 0.5) a.facing = dx > 0 ? 1 : -1;
        }
        a.moving = true;
        a.walk += dt * 8;
        continue;
      }
      if (a.leaving) {
        actors.current.delete(a.id);
        continue;
      }

      if (busy && a.task?.kind === "coffee") a.task = undefined;

      const task = a.task;
      if (task?.kind === "deliver") {
        if (task.stage === "go") {
          task.stage = "talk";
          task.until = now + 2400;
          a.bubble = { text: task.text, until: now + 2400 };
          const target = actors.current.get(task.to);
          if (target) target.bubble = { text: "Ok!", until: now + 1600 };
          const tp = task.to === "chairman" ? l.chairman : (target ?? null);
          if (tp) a.facing = tp.x >= a.x ? 1 : -1;
        } else if (task.stage === "talk" && now > (task.until ?? 0)) {
          task.stage = "back";
          a.carrying = false;
          a.path = route(l, a, home);
        } else if (task.stage === "back") {
          a.task = undefined;
        }
        continue;
      }
      if (task?.kind === "coffee") {
        if (task.stage === "go") {
          task.stage = "sip";
          task.until = now + 3000 + Math.random() * 3000;
          a.bubble = { text: "☕", until: task.until };
        } else if (task.stage === "sip" && now > (task.until ?? 0)) {
          task.stage = "back";
          a.path = route(l, a, home);
        } else if (task.stage === "back") {
          a.task = undefined;
        }
        continue;
      }

      const next = a.queue.shift();
      if (next) {
        a.task = next;
        if (next.kind === "deliver") {
          a.carrying = true;
          const dest = next.to === "chairman" ? visitPoint(l.chairman) : visitPoint(homeOf(st, l, next.to));
          a.path = route(l, a, dest);
        } else {
          a.path = route(l, a, { x: l.coffee.x - 40 - Math.random() * 100, y: l.coffee.y + 40 + Math.random() * 60 });
        }
        continue;
      }

      if (Math.hypot(a.x - home.x, a.y - home.y) > 1) {
        a.path = route(l, a, home);
        continue;
      }
      a.facing = 1;
      if (!busy && !meeting.current && (!rt || rt.status === "idle") && Math.random() < dt / 45) {
        a.queue.push({ kind: "coffee", stage: "go" });
      }
    }
  }

  // ---------------- pixel world (office pixels) ----------------

  function drawWorld(ctx: Ctx, st: CompanyState, l: Layout, now: number) {
    const PW = p(l.W);
    const PH = p(l.H);
    ctx.imageSmoothingEnabled = false;
    rect(ctx, 0, 0, PW, PH, C.night);
    for (let y = 0; y < PH; y += 4) for (let x = (y / 4) % 2 ? 2 : 0; x < PW; x += 7) px(ctx, x, y, "#20243a");

    drawHallway(ctx, l, now);
    for (const z of l.zones) drawRoom(ctx, l, z, now, st);

    const byId = new Map(st.employees.map((e) => [e.id, e]));
    type Item = { y: number; draw: () => void };
    const items: Item[] = [];

    for (const [id, d] of l.desks) {
      const e = byId.get(id);
      if (!e) continue;
      const a = actors.current.get(id);
      const atDesk = !!a && !a.moving && Math.hypot(a.x - d.x, a.y - d.y) < 2;
      const status = st.runtime[id]?.status;
      const glow = atDesk && (status === "thinking" || status === "working");
      items.push({ y: d.y - 1, draw: () => chair(ctx, p(d.x), p(d.y) - SEAT) });
      items.push({ y: d.y + 1, draw: () => desk(ctx, p(d.x), p(d.y), e.role === "ceo", glow, shade(e.color, 0.1), now) });
    }
    const ch = l.chairman;
    items.push({ y: ch.y - 1, draw: () => chair(ctx, p(ch.x), p(ch.y) - SEAT) });
    items.push({ y: ch.y, draw: () => npc(ctx, ch, CHAIRMAN_LOOK, now, false) });
    items.push({ y: ch.y + 1, draw: () => desk(ctx, p(ch.x), p(ch.y), true, Math.sin(now / 3000) > 0, C.yellow, now) });
    const rc = l.receptionist;
    items.push({ y: rc.y, draw: () => npc(ctx, rc, RECEPTION_LOOK, now, true) });
    items.push({ y: rc.y + 1, draw: () => receptionCounter(ctx, p(rc.x), p(rc.y)) });

    for (const a of actors.current.values()) {
      const d = l.desks.get(a.id);
      const seated = !a.moving && !a.leaving && !!d && Math.hypot(a.x - d.x, a.y - d.y) < 2;
      items.push({ y: a.y, draw: () => drawPerson(ctx, a, st.runtime[a.id]?.status ?? "idle", now, seated) });
    }
    items.sort((m, n) => m.y - n.y);
    for (const it of items) it.draw();
  }

  function npc(ctx: Ctx, at: P, look: Look, now: number, typing: boolean) {
    const frame: Frame = typing && Math.floor(now / 900) % 3 === 0 && Math.floor(now / 160) % 2 ? "type" : "stand";
    const s = characterSprite(look, frame, 1);
    ctx.drawImage(s, p(at.x) - 7, p(at.y) - SEAT - s.height + 1);
  }

  function drawHallway(ctx: Ctx, l: Layout, now: number) {
    const hy = p(l.hallY);
    const PW = p(l.W);
    const top = hy - 16;
    for (let x = 6; x < PW - 6; x += 10)
      for (let y = top; y < top + 32; y += 8) rect(ctx, x, y, 10, 8, ((x - 6) / 10 + (y - top) / 8) % 2 ? "#3f4a6e" : "#3a4466");
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

  function floorTiles(ctx: Ctx, z: Zone, x0: number, y0: number, w: number, h: number) {
    switch (z.id) {
      case "chairman":
        rect(ctx, x0, y0, w, h, "#5b2a3a");
        for (let y = y0; y < y0 + h; y += 6) for (let x = x0 + ((y / 6) % 2) * 3; x < x0 + w; x += 6) px(ctx, x, y, "#6b3445");
        break;
      case "ceo":
        for (let y = y0; y < y0 + h; y += 5) {
          rect(ctx, x0, y, w, 5, (y / 5) % 2 ? C.wood3 : shade(C.wood3, -0.06));
          rect(ctx, x0, y + 4, w, 1, C.wood2);
          const off = ((y / 5) % 3) * 13;
          for (let x = x0 + off; x < x0 + w; x += 38) rect(ctx, x, y, 1, 4, C.wood2);
        }
        break;
      case "meeting":
        rect(ctx, x0, y0, w, h, "#3c5a8a");
        for (let y = y0; y < y0 + h; y += 4) for (let x = x0 + ((y / 4) % 2) * 2; x < x0 + w; x += 4) px(ctx, x, y, "#46679a");
        break;
      case "pantry":
        for (let y = y0; y < y0 + h; y += 8) for (let x = x0; x < x0 + w; x += 8) rect(ctx, x, y, 8, 8, ((x - x0) / 8 + (y - y0) / 8) % 2 ? C.white : "#c9d6df");
        break;
      case "lobby":
        for (let y = y0; y < y0 + h; y += 10)
          for (let x = x0; x < x0 + w; x += 10) {
            rect(ctx, x, y, 10, 10, ((x - x0) / 10 + (y - y0) / 10) % 2 ? "#d8c8a8" : "#cdbb98");
            px(ctx, x, y, "#b9a680");
          }
        break;
      case "work":
        for (let y = y0; y < y0 + h; y += 12)
          for (let x = x0; x < x0 + w; x += 12) {
            rect(ctx, x, y, 12, 12, ((x - x0) / 12 + (y - y0) / 12) % 2 ? "#4b5a7a" : "#475574");
            px(ctx, x + 3, y + 4, "#56678a");
            px(ctx, x + 8, y + 9, "#56678a");
          }
        break;
      case "design":
        rect(ctx, x0, y0, w, h, "#e9dcc9");
        for (let y = y0; y < y0 + h; y += 6) rect(ctx, x0, y, w, 1, "#dccdb6");
        break;
      case "qa":
        for (let y = y0; y < y0 + h; y += 10)
          for (let x = x0; x < x0 + w; x += 10) {
            rect(ctx, x, y, 10, 10, "#3f6b66");
            rect(ctx, x, y, 10, 1, "#36605b");
            rect(ctx, x, y, 1, 10, "#36605b");
          }
        break;
      case "devops":
        for (let y = y0; y < y0 + h; y += 8)
          for (let x = x0; x < x0 + w; x += 8) {
            rect(ctx, x, y, 8, 8, "#2a2f45");
            rect(ctx, x, y, 8, 1, "#232739");
            px(ctx, x + 4, y + 4, "#363c58");
          }
        break;
      case "marketing":
        for (let y = y0; y < y0 + h; y += 8)
          for (let x = x0; x < x0 + w; x += 8) rect(ctx, x, y, 8, 8, ((x - x0) / 8 + (y - y0) / 8) % 2 ? "#f2a477" : "#ec9465");
        break;
      case "sales":
        rect(ctx, x0, y0, w, h, "#2f5a46");
        for (let y = y0; y < y0 + h; y += 4) for (let x = x0 + ((y / 4) % 2) * 2; x < x0 + w; x += 4) px(ctx, x, y, "#386a53");
        break;
      case "support":
        for (let y = y0; y < y0 + h; y += 10)
          for (let x = x0; x < x0 + w; x += 10) {
            rect(ctx, x, y, 10, 10, "#4a4f7a");
            px(ctx, x + 5, y + 5, "#5a6090");
          }
        break;
      case "finance":
        for (let y = y0; y < y0 + h; y += 5) {
          rect(ctx, x0, y, w, 5, (y / 5) % 2 ? "#7c6a4e" : "#756347");
          rect(ctx, x0, y + 4, w, 1, "#5e4f38");
        }
        break;
      case "backoffice":
        rect(ctx, x0, y0, w, h, "#55607e");
        for (let y = y0; y < y0 + h; y += 6) for (let x = x0 + ((y / 6) % 2) * 3; x < x0 + w; x += 6) px(ctx, x, y, "#5e6a8a");
        break;
    }
  }

  function drawRoom(ctx: Ctx, l: Layout, z: Zone, now: number, st: CompanyState) {
    const x = p(z.x);
    const y = p(z.y);
    const w = p(z.w);
    const h = p(z.h);
    const BAND = 14;
    const doorX = p(z.door.x);
    const wallFace = z.id === "chairman" ? "#d9c7a0" : z.id === "devops" ? "#8a96ad" : z.topRow ? "#c2c8d6" : "#b8c0d0";
    const cap = C.ink;

    ctx.save();
    ctx.beginPath();
    ctx.rect(x, y, w, h);
    ctx.clip();
    floorTiles(ctx, z, x, y, w, h);
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
      floorTiles(ctx, z, x, y, w, h);
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
    rect(ctx, doorX - 8, z.topRow ? y + h - 7 : y + BAND, 16, 4, C.wood2);

    switch (z.id) {
      case "chairman":
        windowPane(ctx, x + 6, y + 3, 20, 8, now);
        painting(ctx, x + 34, y + 2, 16, 10);
        bookshelf(ctx, x + 62, y + 1);
        rect(ctx, x + 14, y + 26, 72, 40, C.plum);
        rect(ctx, x + 16, y + 28, 68, 36, shade(C.plum, 0.12));
        rect(ctx, doorX - 6, y + 70, 12, h - 73, C.red);
        rect(ctx, doorX - 6, y + 70, 12, 1, C.yellow);
        globe(ctx, x + 86, y + 26, now);
        plant(ctx, x + 6, y + h - 6);
        rect(ctx, x + 6, y + 70, 10, 20, C.outline);
        rect(ctx, x + 7, y + 71, 8, 18, C.wood2);
        rect(ctx, x + 9, y + 74, 4, 4, C.yellow);
        rect(ctx, x + 9, y + 82, 4, 4, C.silver);
        break;
      case "ceo":
        windowPane(ctx, x + 6, y + 3, 22, 8, now);
        windowPane(ctx, x + 34, y + 3, 22, 8, now);
        bookshelf(ctx, x + 64, y + 1);
        rect(ctx, x + 16, y + 30, 68, 44, C.plum);
        rect(ctx, x + 18, y + 32, 64, 40, C.red);
        rect(ctx, x + 20, y + 34, 60, 36, C.plum);
        plant(ctx, x + 6, y + 50);
        plant(ctx, x + w - 8, y + h - 6);
        break;
      case "meeting": {
        windowPane(ctx, x + 8, y + 3, 26, 8, now);
        windowPane(ctx, x + 40, y + 3, 26, 8, now);
        rect(ctx, x + 98, y + 1, 48, 12, C.silver);
        rect(ctx, x + 99, y + 2, 46, 10, C.white);
        if (!meeting.current) {
          rect(ctx, x + 102, y + 5, 14, 1, C.sky);
          rect(ctx, x + 102, y + 8, 22, 1, C.red);
        }
        const mt = l.meeting;
        ellipse(ctx, p(mt.x), p(mt.y) + 2, p(mt.rx) + 1, p(mt.ry) + 1, C.outline);
        ellipse(ctx, p(mt.x), p(mt.y) + 2, p(mt.rx), p(mt.ry), C.wood1);
        ellipse(ctx, p(mt.x), p(mt.y), p(mt.rx), p(mt.ry), C.wood3);
        ellipse(ctx, p(mt.x) - 8, p(mt.y) - 3, p(mt.rx) - 18, p(mt.ry) - 6, C.wood4);
        rect(ctx, p(mt.x) - 6, p(mt.y) - 3, 12, 6, C.ink);
        rect(ctx, p(mt.x) - 5, p(mt.y) - 2, 10, 4, meeting.current ? C.cyan : C.slate);
        rect(ctx, p(mt.x) + 16, p(mt.y) + 1, 6, 4, C.white);
        plant(ctx, x + w - 8, y + 30);
        break;
      }
      case "pantry": {
        windowPane(ctx, x + 8, y + 3, 30, 8, now);
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
        break;
      }
      case "lobby":
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
        rect(ctx, doorX - 10, y + h - 4, 20, 4, C.wood2);
        rect(ctx, doorX - 10, y + h, 20, 2, C.cyan);
        break;
      case "work":
        for (let i = 0; i < 2; i++) windowPane(ctx, x + 8 + i * 30, y + 3, 22, 8, now);
        drawKanban(ctx, x + 130, y + 1, st);
        plant(ctx, x + 6, y + h - 4);
        plant(ctx, x + w - 8, y + h - 4);
        printer(ctx, x + w - 24, y + 20);
        break;
      case "design":
        swatches(ctx, x + 6, y + 4);
        painting(ctx, x + 46, y + 2, 14, 10);
        easel(ctx, x + w - 16, y + 62);
        plant(ctx, x + 8, y + h - 4);
        break;
      case "qa":
        rect(ctx, x + 6, y + 2, 30, 10, C.yellow);
        for (let i = 0; i < 4; i++) rect(ctx, x + 9 + i * 7, y + 4 + (i % 2) * 3, 4, 3, i % 2 ? C.red : C.green);
        windowPane(ctx, x + 44, y + 3, 22, 8, now);
        plant(ctx, x + w - 8, y + h - 4);
        break;
      case "marketing":
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
        for (let i = 0; i < 6; i++) rect(ctx, x + 69 + i * 5, y + 11 - (i + 1) - Math.round(Math.sin(now / 600 + i) + 1), 3, i + 1 + Math.round(Math.sin(now / 600 + i) + 1), i % 2 ? C.orange : C.red);
        plant(ctx, x + w - 8, y + h - 6);
        break;
      case "sales": {
        windowPane(ctx, x + 6, y + 3, 20, 8, now);
        // Sales target board.
        rect(ctx, x + 32, y + 2, 40, 11, C.outline);
        rect(ctx, x + 33, y + 3, 38, 9, C.white);
        const deals = (st.deals ?? []).length;
        for (let i = 0; i < 8; i++) rect(ctx, x + 35 + i * 4, y + 5, 3, 5, i < deals ? C.green : "#d6dde6");
        // Deal bell.
        rect(ctx, x + 82, y + 4, 8, 7, C.yellow);
        rect(ctx, x + 85, y + 2, 2, 2, C.wood1);
        px(ctx, x + 86, y + 11, C.wood1);
        plant(ctx, x + w - 8, y + h - 6);
        break;
      }
      case "support": {
        // Live queue display: one block per open case.
        rect(ctx, x + 6, y + 2, 44, 11, C.outline);
        rect(ctx, x + 7, y + 3, 42, 9, C.night);
        const waiting = (st.cases ?? []).filter((c) => c.status === "open" || c.status === "in_progress").length;
        for (let i = 0; i < Math.min(waiting, 8); i++) rect(ctx, x + 9 + i * 5, y + 5, 3, 5, Math.sin(now / 200 + i) > 0 ? C.red : C.orange);
        if (waiting === 0) rect(ctx, x + 9, y + 7, 38, 1, C.green);
        windowPane(ctx, x + 58, y + 3, 22, 8, now);
        plant(ctx, x + w - 8, y + h - 4);
        break;
      }
      case "finance":
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
        break;
      case "backoffice":
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
        break;
      case "devops":
        dashboard(ctx, x + 6, y + 2, 36, 10, now);
        dashboard(ctx, x + 48, y + 2, 24, 10, now + 900);
        for (let i = 0; i < 3; i++) serverRack(ctx, x + w - 16 - i * 15, y + 20, now + i * 333);
        rect(ctx, x + 2, y + h - 6, w - 4, 2, C.ink);
        px(ctx, x + 20 + ((now / 40) % (w - 40)), y + h - 6, C.lime);
        break;
    }
  }

  function drawKanban(ctx: Ctx, x: number, y: number, st: CompanyState) {
    const pid = selRef.current.project;
    const tickets = st.tickets.filter((t) => t.projectId === pid);
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

  function drawPerson(ctx: Ctx, a: Actor, status: EmpStatus, now: number, seated: boolean) {
    const e = a.emp;
    const look = lookFor(e.id, e.name, e.color, e.role);
    const typing = !a.moving && (status === "working" || status === "thinking");
    let frame: Frame = "stand";
    if (a.moving) frame = Math.floor(a.walk) % 2 ? "walkA" : "walkB";
    else if (typing) frame = Math.floor(now / 160) % 2 ? "type" : "stand";
    const sprite = characterSprite(look, frame, a.facing);
    const fx = p(a.x);
    const fy = p(a.y) - (seated ? SEAT : 0);
    const bob = a.moving && Math.floor(a.walk) % 2 ? 1 : 0;
    if (!seated) ellipse(ctx, fx, fy, 5, 1, "rgba(10,10,20,0.35)");
    if (selRef.current.emp === e.id) {
      ellipse(ctx, fx, p(a.y), 8, 2, C.yellow);
      ellipse(ctx, fx, p(a.y), 6, 1, "rgba(10,10,20,0.35)");
    }
    if (a.leaving) ctx.globalAlpha = 0.85;
    ctx.drawImage(sprite, fx - 7, fy - sprite.height + 1 - bob);
    ctx.globalAlpha = 1;
    if (crownRef.current === e.id) crown(ctx, fx, fy - sprite.height - 4 - bob);
    if (a.carrying) {
      const hx = fx + (a.facing === 1 ? 5 : -10);
      rect(ctx, hx - 1, fy - 12 - bob, 7, 8, C.outline);
      rect(ctx, hx, fy - 11 - bob, 5, 6, C.white);
      rect(ctx, hx + 1, fy - 9 - bob, 3, 1, C.slate);
      rect(ctx, hx + 1, fy - 7 - bob, 2, 1, C.slate);
    }
  }

  // ---------------- composition + crisp text overlay ----------------

  function toScreen(wx: number, wy: number) {
    const v = view.current;
    return { x: v.ox + wx * v.scale, y: v.oy + wy * v.scale };
  }

  function compose(ctx: CanvasRenderingContext2D, off: HTMLCanvasElement, st: CompanyState, l: Layout, now: number) {
    const v = view.current;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.imageSmoothingEnabled = false;
    ctx.fillStyle = C.night;
    ctx.fillRect(0, 0, ctx.canvas.width, ctx.canvas.height);
    if (Number.isInteger(v.k)) {
      ctx.drawImage(off, v.ox, v.oy, off.width * v.k, off.height * v.k);
    } else {
      // Sharp-bilinear: nearest-neighbour upscale to the next integer, then a smooth downscale.
      const n = Math.ceil(v.k);
      bigCanvas ??= document.createElement("canvas");
      const big = bigCanvas;
      if (big.width !== off.width * n || big.height !== off.height * n) {
        big.width = off.width * n;
        big.height = off.height * n;
      }
      const b = big.getContext("2d")!;
      b.imageSmoothingEnabled = false;
      b.drawImage(off, 0, 0, big.width, big.height);
      ctx.imageSmoothingEnabled = true;
      ctx.imageSmoothingQuality = "high";
      ctx.drawImage(big, v.ox, v.oy, off.width * v.k, off.height * v.k);
      ctx.imageSmoothingEnabled = false;
    }

    const fs = Math.max(12, Math.round(v.k * 6));
    for (const z of l.zones) {
      const s = toScreen(z.x + 6, z.y + 30);
      tag(ctx, s.x, s.y, z.label.toUpperCase(), fs * 0.9, "rgba(20,18,31,0.75)", C.yellow, "left");
    }
    if (meeting.current?.topic) {
      const mz = l.zones.find((z) => z.id === "meeting")!;
      const s = toScreen(mz.x + mz.w - 50 * U, mz.y + 4);
      tag(ctx, s.x, s.y, meeting.current.topic, fs * 0.85, C.white, C.outline, "center");
    }
    const chs = toScreen(l.chairman.x, l.chairman.y + 16);
    tag(ctx, chs.x, chs.y, "Bạn·Chủ tịch", fs, C.yellow, C.outline, "center");
    const rcs = toScreen(l.receptionist.x, l.receptionist.y + 16);
    tag(ctx, rcs.x, rcs.y, "Lễ tân", fs, "rgba(20,18,31,0.85)", C.cyan, "center");

    for (const a of actors.current.values()) {
      const s = toScreen(a.x, a.y + 16);
      const sel = selRef.current.emp === a.id;
      tag(ctx, s.x, s.y, `${a.emp.name}·${ROLE_LABEL[a.emp.role]}`, fs, sel ? C.yellow : "rgba(20,18,31,0.85)", sel ? C.outline : C.white, "center");
    }
    for (const a of actors.current.values()) bubble(ctx, a, st.runtime[a.id]?.status ?? "idle", now, fs);

    const m = mouse.current;
    if (m && !drag.current?.moved) {
      const hit = hitTest(m);
      if (hit) {
        const e = hit.emp;
        const rt = st.runtime[hit.id];
        const s = toScreen(hit.x + 18, hit.y - 60);
        panel(ctx, s.x, s.y, [`${e.name} · ${ROLE_LABEL[e.role]}`, e.provider === "mock" ? "Mock (giả lập)" : e.model, rt?.note || (hit.leaving ? "Đang rời công ty" : "Đang rảnh")], fs);
      }
    }
    tag(
      ctx,
      8,
      ctx.canvas.height - fs - 8,
      cam.current.manual ? "Lăn chuột: zoom · Kéo: di chuyển · Nhấp đúp: vừa màn hình" : "Lăn chuột để phóng to · kéo để di chuyển",
      fs * 0.85,
      "rgba(20,18,31,0.7)",
      C.silver,
      "left",
    );
  }

  function tag(ctx: CanvasRenderingContext2D, x: number, y: number, text: string, size: number, bg: string, fg: string, align: "left" | "center" | "right") {
    ctx.font = `${Math.round(size)}px ${FONT}`;
    const w = Math.ceil(ctx.measureText(text).width) + 8;
    const h = Math.round(size * 0.95);
    const left = Math.round(align === "center" ? x - w / 2 : align === "right" ? x - w : x);
    const top = Math.round(y);
    ctx.fillStyle = bg;
    ctx.fillRect(left, top, w, h);
    ctx.fillStyle = fg;
    ctx.textAlign = "left";
    ctx.textBaseline = "middle";
    ctx.fillText(text, left + 4, top + h / 2 + 1);
  }

  function bubble(ctx: CanvasRenderingContext2D, a: Actor, status: EmpStatus, now: number, fs: number) {
    const DOTS = "\u0000";
    let text = "";
    let bg: string = C.white;
    let fg: string = C.outline;
    if (a.bubble && now < a.bubble.until) {
      text = a.bubble.text.length > 30 ? a.bubble.text.slice(0, 29) + "…" : a.bubble.text;
    } else if (!a.moving) {
      if (status === "thinking") text = DOTS;
      else if (status === "working") text = Math.floor(now / 300) % 2 ? "</>" : "< >";
      else if (status === "waiting") text = "z z";
      else if (status === "blocked") {
        text = " ! ";
        bg = C.red;
        fg = C.white;
      }
    }
    if (!text) return;
    const v = view.current;
    const unit = Math.max(1, Math.round(v.k));
    ctx.font = `${fs}px ${FONT}`;
    const w = text === DOTS ? 12 * unit : Math.ceil(ctx.measureText(text).width) + 6 * unit;
    const h = Math.round(fs) + 2 * unit;
    const s = toScreen(a.x, a.y - (crownRef.current === a.id ? 62 : 50));
    const left = Math.round(s.x - w / 2);
    const top = Math.round(s.y - h);
    ctx.fillStyle = C.outline;
    ctx.fillRect(left - unit, top, w + 2 * unit, h);
    ctx.fillRect(left, top - unit, w, h + 2 * unit);
    ctx.fillStyle = bg;
    ctx.fillRect(left, top, w, h);
    ctx.fillStyle = C.outline;
    ctx.fillRect(Math.round(s.x) - unit, top + h + unit, 3 * unit, unit);
    ctx.fillRect(Math.round(s.x), top + h + 2 * unit, unit, unit);
    ctx.fillStyle = bg;
    ctx.fillRect(Math.round(s.x) - unit, top + h, 3 * unit, unit);
    if (text === DOTS) {
      const n = 1 + (Math.floor(now / 350) % 3);
      for (let i = 0; i < 3; i++) {
        ctx.fillStyle = i < n ? fg : "#c2c8d6";
        ctx.fillRect(Math.round(left + w / 2 - 4 * unit + i * 3 * unit), Math.round(top + h / 2 - unit), 2 * unit, 2 * unit);
      }
      return;
    }
    ctx.fillStyle = fg;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(text, left + w / 2, top + h / 2 + 1);
  }

  function panel(ctx: CanvasRenderingContext2D, x: number, y: number, lines: string[], fs: number) {
    ctx.font = `${fs}px ${FONT}`;
    const clip = (l: string) => (l.length > 48 ? l.slice(0, 47) + "…" : l);
    const w = Math.max(...lines.map((l) => ctx.measureText(clip(l)).width)) + 16;
    const lh = Math.round(fs * 1.05);
    const h = lines.length * lh + 10;
    const left = Math.round(Math.min(x, ctx.canvas.width - w - 8));
    const top = Math.round(Math.max(8, y));
    ctx.fillStyle = C.outline;
    ctx.fillRect(left - 2, top - 2, w + 4, h + 4);
    ctx.fillStyle = C.ink;
    ctx.fillRect(left, top, w, h);
    ctx.fillStyle = C.navy;
    ctx.fillRect(left, top, w, lh + 4);
    ctx.textAlign = "left";
    ctx.textBaseline = "top";
    lines.forEach((l, i) => {
      ctx.fillStyle = i === 0 ? C.yellow : C.white;
      ctx.fillText(clip(l), left + 8, top + 4 + i * lh);
    });
  }

  function hitTest(pt: P): Actor | null {
    let best: Actor | null = null;
    let bd = 22;
    for (const a of actors.current.values()) {
      const d = Math.hypot(pt.x - a.x, pt.y - (a.y - 18));
      if (d < bd) {
        bd = d;
        best = a;
      }
    }
    return best;
  }

  function devicePoint(ev: React.MouseEvent | React.WheelEvent) {
    const r = canvasRef.current!.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    return { x: (ev.clientX - r.left) * dpr, y: (ev.clientY - r.top) * dpr };
  }

  function toWorld(ev: React.MouseEvent): P {
    const d = devicePoint(ev);
    const { scale, ox, oy } = view.current;
    return { x: (d.x - ox) / scale, y: (d.y - oy) / scale };
  }

  return (
    <div className="office" ref={wrapRef}>
      <canvas
        ref={canvasRef}
        onWheel={(ev) => {
          const c = cam.current;
          const v = view.current;
          const d = devicePoint(ev);
          // The office pixel under the cursor stays put while zooming.
          const ux = (d.x - v.ox) / v.k;
          const uy = (d.y - v.oy) / v.k;
          const cur = c.manual ? c.zoom : Math.max(1, Math.round(v.k));
          const next = Math.min(6, Math.max(1, cur + (ev.deltaY < 0 ? 1 : -1)));
          c.manual = true;
          c.zoom = next;
          const canvas = canvasRef.current!;
          c.cx = ux - (d.x - canvas.width / 2) / next;
          c.cy = uy - (d.y - canvas.height / 2) / next;
        }}
        onMouseDown={(ev) => {
          const d = devicePoint(ev);
          const c = cam.current;
          if (!c.manual) {
            const v = view.current;
            c.cx = (canvasRef.current!.width / 2 - v.ox) / v.k;
            c.cy = (canvasRef.current!.height / 2 - v.oy) / v.k;
          }
          drag.current = { x: d.x, y: d.y, cx: c.cx, cy: c.cy, moved: false };
        }}
        onMouseMove={(ev) => {
          mouse.current = toWorld(ev);
          const g = drag.current;
          if (g && ev.buttons & 1) {
            const d = devicePoint(ev);
            if (Math.hypot(d.x - g.x, d.y - g.y) > 6) g.moved = true;
            if (g.moved) {
              const c = cam.current;
              if (!c.manual) {
                c.manual = true;
                c.zoom = Math.max(1, Math.round(view.current.k));
              }
              c.cx = g.cx - (d.x - g.x) / c.zoom;
              c.cy = g.cy - (d.y - g.y) / c.zoom;
            }
          }
          canvasRef.current!.style.cursor = drag.current?.moved ? "grabbing" : hitTest(mouse.current) ? "pointer" : "grab";
        }}
        onMouseUp={(ev) => {
          const g = drag.current;
          drag.current = null;
          if (g?.moved) return;
          const hit = hitTest(toWorld(ev));
          setSelectedEmployee(hit ? hit.id : null);
        }}
        onDoubleClick={() => {
          cam.current.manual = false;
        }}
        onMouseLeave={() => {
          mouse.current = null;
          drag.current = null;
        }}
      />
    </div>
  );
}
