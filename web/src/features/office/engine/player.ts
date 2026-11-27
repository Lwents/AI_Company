import { walkable, type Layout } from "../layout";
import { SPEED, p, type Scene } from "./scene";

/** How close (world units) the player must be to talk to someone. */
export const TALK_RANGE = 46;

/** Moves the chairman from the held keys, keeps the camera on them and finds who is in reach. */
export function movePlayer(scene: Scene, l: Layout, dt: number) {
  scene.player ??= { x: l.chairman.x, y: l.chairman.y, facing: 1, walk: 0, moving: false };
  const pl = scene.player;
  const k = scene.keys;
  let dx = (k.right ? 1 : 0) - (k.left ? 1 : 0);
  let dy = (k.down ? 1 : 0) - (k.up ? 1 : 0);
  if (scene.frozen) dx = dy = 0;
  pl.moving = false;
  if (dx || dy) {
    const len = Math.hypot(dx, dy);
    const step = SPEED * 1.15 * dt;
    const nx = pl.x + (dx / len) * step;
    const ny = pl.y + (dy / len) * step;
    // Move each axis separately so the player slides along walls instead of sticking.
    if (walkable(l, nx, pl.y)) pl.x = nx;
    if (walkable(l, pl.x, ny)) pl.y = ny;
    if (dx) pl.facing = dx > 0 ? 1 : -1;
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
