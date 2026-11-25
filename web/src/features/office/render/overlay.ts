import type { CompanyState, EmpStatus } from "@/api/types";
import { ROLE_LABEL } from "@/shared/labels";
import type { Layout } from "../layout";
import { FONT, U, type Actor, type Scene } from "../engine/scene";
import { hitTest, toScreen } from "../engine/camera";
import { C } from "../sprites/draw";

/** Scratch canvas for sharp-bilinear scaling at non-integer zoom. */
let bigCanvas: HTMLCanvasElement | null = null;

/**
 * Scales the pixel canvas onto the screen, then draws text (labels, bubbles, hover card)
 * at screen resolution so it stays crisp at any zoom.
 */
export function compose(ctx: CanvasRenderingContext2D, off: HTMLCanvasElement, scene: Scene, st: CompanyState, l: Layout, now: number) {
  const v = scene.view;
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
    const s = toScreen(scene, z.x + 6, z.y + 30);
    tag(ctx, s.x, s.y, z.label.toUpperCase(), fs * 0.9, "rgba(20,18,31,0.75)", C.yellow, "left");
  }
  if (scene.meeting?.topic) {
    const mz = l.zones.find((z) => z.id === "meeting")!;
    const s = toScreen(scene, mz.x + mz.w - 50 * U, mz.y + 4);
    tag(ctx, s.x, s.y, scene.meeting.topic, fs * 0.85, C.white, C.outline, "center");
  }
  const chs = toScreen(scene, l.chairman.x, l.chairman.y + 16);
  tag(ctx, chs.x, chs.y, "Bạn·Chủ tịch", fs, C.yellow, C.outline, "center");
  const rcs = toScreen(scene, l.receptionist.x, l.receptionist.y + 16);
  tag(ctx, rcs.x, rcs.y, "Lễ tân", fs, "rgba(20,18,31,0.85)", C.cyan, "center");

  for (const a of scene.actors.values()) {
    const s = toScreen(scene, a.x, a.y + 16);
    const sel = scene.selectedEmployee === a.id;
    tag(ctx, s.x, s.y, `${a.emp.name}·${ROLE_LABEL[a.emp.role]}`, fs, sel ? C.yellow : "rgba(20,18,31,0.85)", sel ? C.outline : C.white, "center");
  }
  for (const a of scene.actors.values()) bubble(ctx, scene, a, st.runtime[a.id]?.status ?? "idle", now, fs);

  const m = scene.mouse;
  if (m && !scene.drag?.moved) {
    const hit = hitTest(scene, m);
    if (hit) {
      const e = hit.emp;
      const rt = st.runtime[hit.id];
      const s = toScreen(scene, hit.x + 18, hit.y - 60);
      hoverCard(ctx, s.x, s.y, [`${e.name} · ${ROLE_LABEL[e.role]}`, e.provider === "mock" ? "Mock (giả lập)" : e.model, rt?.note || (hit.leaving ? "Đang rời công ty" : "Đang rảnh")], fs);
    }
  }
  tag(
    ctx,
    8,
    ctx.canvas.height - fs - 8,
    scene.cam.manual ? "Lăn chuột: zoom · Kéo: di chuyển · Nhấp đúp: vừa màn hình" : "Lăn chuột để phóng to · kéo để di chuyển",
    fs * 0.85,
    "rgba(20,18,31,0.7)",
    C.silver,
    "left",
  );
}

/** Text on a filled label. */
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

/** Speech bubble above a person: a message, or an icon for their current status. */
function bubble(ctx: CanvasRenderingContext2D, scene: Scene, a: Actor, status: EmpStatus, now: number, fs: number) {
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
  const unit = Math.max(1, Math.round(scene.view.k));
  ctx.font = `${fs}px ${FONT}`;
  const w = text === DOTS ? 12 * unit : Math.ceil(ctx.measureText(text).width) + 6 * unit;
  const h = Math.round(fs) + 2 * unit;
  const s = toScreen(scene, a.x, a.y - (scene.crown === a.id ? 62 : 50));
  const left = Math.round(s.x - w / 2);
  const top = Math.round(s.y - h);
  // Pixel-rounded outline + tail.
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

/** Info card shown when hovering a person. */
function hoverCard(ctx: CanvasRenderingContext2D, x: number, y: number, lines: string[], fs: number) {
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
