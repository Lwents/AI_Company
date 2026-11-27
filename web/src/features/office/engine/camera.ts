import type { InteractiveBox, Layout, P } from "../layout";
import { U, p, type Actor, type Scene } from "./scene";

/** Most office-pixel → device-pixel magnification. Zooming out stops at "fit". */
const MAX_ZOOM = 8;

/** Sizes the canvas to its container and computes the world → screen transform. */
export function fitView(scene: Scene, canvas: HTMLCanvasElement, wrap: HTMLElement, l: Layout) {
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
  const c = scene.cam;
  const fit = Math.min(cw / PW, ch / PH);
  // Integer zoom keeps pixels perfectly square; below 2× use the exact fit.
  const fitK = fit >= 2 ? Math.floor(fit) : fit;
  if (c.manual && c.zoom <= fitK * 1.001) c.manual = false;
  if (!c.manual) {
    const k = fitK;
    scene.view = { k, fitK, scale: k / U, ox: Math.floor((cw - PW * k) / 2), oy: Math.floor((ch - PH * k) / 2) };
    return;
  }
  const k = c.zoom;
  const halfW = cw / k / 2;
  const halfH = ch / k / 2;
  c.cx = PW * k <= cw ? PW / 2 : Math.min(PW - halfW, Math.max(halfW, c.cx));
  c.cy = PH * k <= ch ? PH / 2 : Math.min(PH - halfH, Math.max(halfH, c.cy));
  scene.view = { k, fitK, scale: k / U, ox: Math.round(cw / 2 - c.cx * k), oy: Math.round(ch / 2 - c.cy * k) };
}

export function toScreen(scene: Scene, wx: number, wy: number) {
  const v = scene.view;
  return { x: v.ox + wx * v.scale, y: v.oy + wy * v.scale };
}

/** Mouse event → device pixels on the canvas. */
export function devicePoint(canvas: HTMLCanvasElement, ev: { clientX: number; clientY: number }): P {
  const r = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  return { x: (ev.clientX - r.left) * dpr, y: (ev.clientY - r.top) * dpr };
}

export function deviceToWorld(scene: Scene, d: P): P {
  const { scale, ox, oy } = scene.view;
  return { x: (d.x - ox) / scale, y: (d.y - oy) / scale };
}

/** Closest person under a world point (their body, not their feet). */
export function hitTest(scene: Scene, pt: P): Actor | null {
  let best: Actor | null = null;
  let bd = 22;
  for (const a of scene.actors.values()) {
    const d = Math.hypot(pt.x - a.x, pt.y - (a.y - 18));
    if (d < bd) {
      bd = d;
      best = a;
    }
  }
  return best;
}

/** Interactive bookshelf under a world point (includes comfortable margin and pinned tag below). */
export function hitTestBookshelf(scene: Scene, pt: P): InteractiveBox | null {
  const pad = 6;
  for (const b of scene.bookshelves) {
    const extraBottom = b.pinnedTag ? 18 : pad;
    if (pt.x >= b.x - pad && pt.x <= b.x + b.w + pad && pt.y >= b.y - pad && pt.y <= b.y + b.h + extraBottom) {
      return b;
    }
  }
  return null;
}

/**
 * Multiplies the zoom by `factor`, keeping the office pixel under device point `d` in place.
 * Zooming out to (or past) the fit level returns to auto-fit.
 */
export function zoomBy(scene: Scene, canvas: HTMLCanvasElement, d: P, factor: number) {
  const c = scene.cam;
  const v = scene.view;
  const ux = (d.x - v.ox) / v.k;
  const uy = (d.y - v.oy) / v.k;
  // No snapping to whole numbers: it would trap small touchpad steps. Non-integer zoom
  // still looks crisp thanks to sharp-bilinear scaling in render/overlay.ts.
  const next = Math.min(MAX_ZOOM, Math.max(v.fitK, v.k * factor));
  if (next <= v.fitK * 1.001) {
    c.manual = false;
    return;
  }
  c.manual = true;
  c.zoom = next;
  c.cx = ux - (d.x - canvas.width / 2) / next;
  c.cy = uy - (d.y - canvas.height / 2) / next;
}

/** Mouse wheel / touchpad (pinch arrives as a wheel event with ctrlKey) → smooth zoom factor. */
export function wheelFactor(ev: WheelEvent): number {
  const lines = ev.deltaMode === 1 ? 16 : ev.deltaMode === 2 ? 400 : 1;
  const dy = Math.max(-200, Math.min(200, ev.deltaY * lines));
  return Math.exp(-dy * (ev.ctrlKey ? 0.01 : 0.002));
}

/** Zoom buttons: one step around the canvas centre. */
export function zoomStep(scene: Scene, canvas: HTMLCanvasElement, zoomIn: boolean) {
  zoomBy(scene, canvas, { x: canvas.width / 2, y: canvas.height / 2 }, zoomIn ? 1.25 : 0.8);
}

export function startDrag(scene: Scene, canvas: HTMLCanvasElement, d: P) {
  const c = scene.cam;
  if (!c.manual) {
    const v = scene.view;
    c.cx = (canvas.width / 2 - v.ox) / v.k;
    c.cy = (canvas.height / 2 - v.oy) / v.k;
  }
  scene.drag = { x: d.x, y: d.y, cx: c.cx, cy: c.cy, moved: false };
}

/** Pans once the pointer has moved a few pixels (smaller moves still count as a click). */
export function dragTo(scene: Scene, d: P) {
  const g = scene.drag;
  if (!g) return;
  if (Math.hypot(d.x - g.x, d.y - g.y) > 6) g.moved = true;
  if (!g.moved) return;
  scene.follow = false;
  const c = scene.cam;
  if (!c.manual) {
    c.manual = true;
    c.zoom = scene.view.k;
  }
  c.cx = g.cx - (d.x - g.x) / c.zoom;
  c.cy = g.cy - (d.y - g.y) / c.zoom;
}

export function resetCamera(scene: Scene) {
  scene.cam.manual = false;
  scene.follow = false;
}
