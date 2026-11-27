import { route, visitPoint, walkable, type Layout, type P } from "../layout";
import { SPEED, p, type Scene } from "./scene";

/** How close (world units) the chairman must be to talk to someone. */
export const TALK_RANGE = 46;
/** Give up following someone after this many re-routes (they keep walking away). */
const MAX_TRIES = 12;

function ensurePlayer(scene: Scene, l: Layout) {
  scene.player ??= { x: l.chairman.x, y: l.chairman.y, facing: 1, walk: 0, moving: false, path: [], rerouteAt: 0 };
  return scene.player;
}

/** Where to stand to talk to an employee: beside their desk when seated there, else next to them. */
function besideOf(scene: Scene, l: Layout, id: string): P | null {
  const a = scene.actors.get(id);
  if (!a) return null;
  const desk = l.desks.get(id);
  if (desk && !a.moving && Math.hypot(a.x - desk.x, a.y - desk.y) < 2) return visitPoint(desk);
  return { x: a.x + 26, y: a.y + 6 };
}

/** Click / tap on an employee: the chairman walks over (through doors and the hallway), then talks. */
export function walkToEmployee(scene: Scene, l: Layout, id: string) {
  const pl = ensurePlayer(scene, l);
  const to = besideOf(scene, l, id);
  if (!to) return;
  pl.path = route(l, pl, to);
  pl.rerouteAt = 0;
  scene.talkTarget = id;
  scene.talkTries = 0;
}

/** Click / tap on the floor: walk there when it is a place one can stand. */
export function walkToPoint(scene: Scene, l: Layout, to: P) {
  if (!walkable(l, to.x, to.y)) return false;
  const pl = ensurePlayer(scene, l);
  pl.path = route(l, pl, to);
  scene.talkTarget = null;
  return true;
}

/** Moves the chairman along a clicked route, keeps the camera on them and finds who is in reach. */
export function movePlayer(scene: Scene, l: Layout, dt: number, now: number) {
  const pl = ensurePlayer(scene, l);
  pl.moving = false;
  const target = scene.talkTarget ? scene.actors.get(scene.talkTarget) : undefined;
  if (scene.talkTarget && !target) scene.talkTarget = null;
  if (target && Math.hypot(target.x - pl.x, target.y - pl.y) <= TALK_RANGE) {
    // Close enough: stop and talk (no need to finish the route).
    pl.path = [];
    pl.facing = target.x >= pl.x ? 1 : -1;
    scene.arrived = target.id;
    scene.talkTarget = null;
  } else if (target && !scene.frozen && (pl.path.length === 0 || (target.moving && now > pl.rerouteAt))) {
    // They moved (or the route ended short): head for where they are now.
    if (scene.talkTries >= MAX_TRIES) scene.talkTarget = null;
    else {
      scene.talkTries += 1;
      const to = besideOf(scene, l, target.id);
      if (to) pl.path = route(l, pl, to);
      pl.rerouteAt = now + 1000;
    }
  }
  if (pl.path.length && !scene.frozen) {
    const step = SPEED * 1.15 * dt;
    const t = pl.path[0];
    const dx = t.x - pl.x;
    const dy = t.y - pl.y;
    const d = Math.hypot(dx, dy);
    if (d <= step) {
      pl.x = t.x;
      pl.y = t.y;
      pl.path.shift();
    } else {
      pl.x += (dx / d) * step;
      pl.y += (dy / d) * step;
      if (Math.abs(dx) > 0.5) pl.facing = dx > 0 ? 1 : -1;
    }
    pl.moving = true;
    pl.walk += dt * 9;
    // Walking zooms in on the chairman and follows them around.
    if (!scene.cam.manual) {
      scene.cam.manual = true;
      scene.cam.zoom = Math.min(8, Math.max(3, scene.view.fitK * 2));
    }
    scene.follow = true;
  }
  if (scene.follow) {
    if (!scene.cam.manual) scene.follow = false;
    else {
      scene.cam.cx = p(pl.x);
      scene.cam.cy = p(pl.y) - 12;
    }
  }
  // Closest employee within reach (not someone walking out of the building).
  let best: string | null = null;
  let bd = TALK_RANGE;
  for (const a of scene.actors.values()) {
    if (a.leaving) continue;
    const d = Math.hypot(a.x - pl.x, a.y - pl.y);
    if (d < bd) {
      bd = d;
      best = a.id;
    }
  }
  scene.nearby = best;
}

/** The chairman is sitting at their desk (not walked away from it). */
export function playerSeated(scene: Scene, l: Layout): boolean {
  const pl = scene.player;
  return !pl || (!pl.moving && Math.hypot(pl.x - l.chairman.x, pl.y - l.chairman.y) < 3);
}
