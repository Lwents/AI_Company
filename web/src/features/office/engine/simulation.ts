import type { CompanyState } from "@/api/types";
import type { LiveEvent } from "@/store";
import { meetingSeats, route, visitPoint, type Layout, type P } from "../layout";
import { SPEED, type Scene } from "./scene";

/** Where an employee belongs right now: their meeting seat, else their desk. */
export function homeOf(scene: Scene, st: CompanyState, l: Layout, id: string): P {
  const m = scene.meeting;
  if (m) {
    const present = m.participants.filter((x) => st.employees.some((e) => e.id === x && e.enabled));
    const i = present.indexOf(id);
    if (i >= 0) return meetingSeats(l, present.length)[i];
  }
  return l.desks.get(id) ?? { x: l.entrance.x, y: l.hallY };
}

/** Feeds a live server event (handoff / meeting) into the animation. */
export function applyLiveEvent(scene: Scene, e: LiveEvent) {
  if (e.type === "meeting") {
    scene.meeting = e.active ? { participants: e.participants, topic: e.topic } : null;
    return;
  }
  if (e.from === "customer") {
    const t = scene.actors.get(e.to);
    if (t) t.bubble = { text: e.text || "☎", until: performance.now() + 3500 };
    return;
  }
  const a = scene.actors.get(e.from);
  if (!a || a.leaving) return;
  if (a.task?.kind === "coffee") {
    a.task = undefined;
    a.path = [];
  }
  if (a.queue.length < 4) a.queue.push({ kind: "deliver", to: e.to, text: e.text, stage: "go" });
}

/** Keeps actors in sync with the staff list: new hires walk in, people who left walk out. */
function syncStaff(scene: Scene, st: CompanyState, l: Layout, now: number) {
  const staff = st.employees.filter((e) => e.enabled);
  for (const e of staff) {
    const a = scene.actors.get(e.id);
    if (a) {
      a.emp = e;
      if (a.leaving) {
        a.leaving = false;
        a.path = [];
      }
      continue;
    }
    const d = l.desks.get(e.id)!;
    const fresh = scene.initialized;
    const start = fresh ? l.outside : d;
    scene.actors.set(e.id, {
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
  scene.initialized = true;
  for (const a of scene.actors.values()) {
    if (!a.leaving && !staff.some((e) => e.id === a.id)) {
      a.leaving = true;
      a.task = undefined;
      a.queue = [];
      a.carrying = false;
      a.bubble = { text: "Tạm biệt!", until: now + 3000 };
      a.path = [...route(l, a, l.entrance), l.outside];
    }
  }
}

/** Advances the simulation by dt seconds. */
export function tick(scene: Scene, st: CompanyState, l: Layout, dt: number, now: number) {
  syncStaff(scene, st, l, now);

  for (const a of scene.actors.values()) {
    const rt = st.runtime[a.id];
    const busy = rt && (rt.status === "thinking" || rt.status === "working" || rt.status === "waiting");
    const home = homeOf(scene, st, l, a.id);

    // Walking.
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
      scene.actors.delete(a.id);
      continue;
    }

    if (busy && a.task?.kind === "coffee") a.task = undefined;

    // Current task.
    const task = a.task;
    if (task?.kind === "deliver") {
      if (task.stage === "go") {
        task.stage = "talk";
        task.until = now + 2400;
        a.bubble = { text: task.text, until: now + 2400 };
        const target = scene.actors.get(task.to);
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

    // Next queued task.
    const next = a.queue.shift();
    if (next) {
      a.task = next;
      if (next.kind === "deliver") {
        a.carrying = true;
        const dest = next.to === "chairman" ? visitPoint(l.chairman) : visitPoint(homeOf(scene, st, l, next.to));
        a.path = route(l, a, dest);
      } else {
        a.path = route(l, a, { x: l.coffee.x - 40 - Math.random() * 100, y: l.coffee.y + 40 + Math.random() * 60 });
      }
      continue;
    }

    // Idle: go home, occasionally fetch a coffee.
    if (Math.hypot(a.x - home.x, a.y - home.y) > 1) {
      a.path = route(l, a, home);
      continue;
    }
    a.facing = 1;
    if (!busy && !scene.meeting && (!rt || rt.status === "idle") && Math.random() < dt / 45) {
      a.queue.push({ kind: "coffee", stage: "go" });
    }
  }
}
