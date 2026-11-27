import type { CompanyState, EmpStatus } from "@/api/types";
import type { Layout, P } from "../layout";
import { SEAT, p, type Actor, type Scene } from "../engine/scene";
import { C, ellipse, px, rect, shade, type Ctx } from "../sprites/draw";
import { SPRITE_W, characterSprite, lookFor, type Frame, type Look } from "../sprites/character";
import { chair, crown, desk, receptionCounter } from "../sprites/furniture";
import { drawHallway, drawRoom } from "./rooms";
import { playerSeated } from "../engine/player";

const CHAIRMAN_LOOK: Look = { gender: "male", hair: "#2b2016", skin: "#f2c8a0", shirt: "#ffcd75", pants: "#29366f", style: 0, tie: C.red, glasses: true, skirt: false, accent: C.red };
const RECEPTION_LOOK: Look = { gender: "female", hair: "#7a4a26", skin: "#f7d7bd", shirt: "#73eff7", pants: "#333c57", style: 1, tie: "#f4f4f4", glasses: false, skirt: true, accent: "#ff8fb1" };

/** Paints the whole office into the low-res pixel canvas. */
export function drawWorld(ctx: Ctx, scene: Scene, st: CompanyState, l: Layout, now: number) {
  const PW = p(l.W);
  const PH = p(l.H);
  ctx.imageSmoothingEnabled = false;
  rect(ctx, 0, 0, PW, PH, C.night);
  for (let y = 0; y < PH; y += 4) for (let x = (y / 4) % 2 ? 2 : 0; x < PW; x += 7) px(ctx, x, y, "#20243a");

  drawHallway(ctx, l, now);
  for (const z of l.zones) drawRoom(ctx, l, z, now, st, scene);

  // Furniture and people are depth-sorted by their feet (y), so whoever is lower is drawn in front.
  const byId = new Map(st.employees.map((e) => [e.id, e]));
  const items: Array<{ y: number; draw: () => void }> = [];

  for (const [id, d] of l.desks) {
    const e = byId.get(id);
    if (!e) continue;
    const a = scene.actors.get(id);
    const atDesk = !!a && !a.moving && Math.hypot(a.x - d.x, a.y - d.y) < 2;
    const status = st.runtime[id]?.status;
    const glow = atDesk && (status === "thinking" || status === "working");
    items.push({ y: d.y - 1, draw: () => chair(ctx, p(d.x), p(d.y) - SEAT) });
    items.push({ y: d.y + 1, draw: () => desk(ctx, p(d.x), p(d.y), e.role === "ceo", glow, shade(e.color, 0.1), now) });
  }
  const ch = l.chairman;
  items.push({ y: ch.y - 1, draw: () => chair(ctx, p(ch.x), p(ch.y) - SEAT) });
  const pl = scene.player;
  if (pl && !playerSeated(scene, l)) {
    items.push({ y: pl.y, draw: () => drawPlayer(ctx, scene, now) });
  } else {
    items.push({ y: ch.y, draw: () => npc(ctx, ch, CHAIRMAN_LOOK, now, false) });
  }
  items.push({ y: ch.y + 1, draw: () => desk(ctx, p(ch.x), p(ch.y), true, Math.sin(now / 3000) > 0, C.yellow, now) });
  const rc = l.receptionist;
  items.push({ y: rc.y, draw: () => npc(ctx, rc, RECEPTION_LOOK, now, true) });
  items.push({ y: rc.y + 1, draw: () => receptionCounter(ctx, p(rc.x), p(rc.y)) });

  for (const a of scene.actors.values()) {
    const d = l.desks.get(a.id);
    const seated = !a.moving && !a.leaving && !!d && Math.hypot(a.x - d.x, a.y - d.y) < 2;
    items.push({ y: a.y, draw: () => drawPerson(ctx, scene, a, st.runtime[a.id]?.status ?? "idle", now, seated) });
  }
  items.sort((m, n) => m.y - n.y);
  for (const it of items) it.draw();
}

/** Non-employee characters (chairman, receptionist). */
function npc(ctx: Ctx, at: P, look: Look, now: number, typing: boolean) {
  const frame: Frame = typing && Math.floor(now / 900) % 3 === 0 && Math.floor(now / 160) % 2 ? "type" : "stand";
  const s = characterSprite(look, frame, 1);
  ctx.drawImage(s, p(at.x) - SPRITE_W / 2, p(at.y) - SEAT - s.height + 1);
}

function drawPerson(ctx: Ctx, scene: Scene, a: Actor, status: EmpStatus, now: number, seated: boolean) {
  const e = a.emp;
  const look = lookFor(e.id, e.name, e.color, e.role, e.gender);
  const typing = !a.moving && (status === "working" || status === "thinking");
  let frame: Frame = "stand";
  if (a.moving) frame = Math.floor(a.walk) % 2 ? "walkA" : "walkB";
  else if (typing) frame = Math.floor(now / 160) % 2 ? "type" : "stand";
  const sprite = characterSprite(look, frame, a.facing);
  const fx = p(a.x);
  const fy = p(a.y) - (seated ? SEAT : 0);
  const bob = a.moving && Math.floor(a.walk) % 2 ? 1 : 0;
  if (!seated) ellipse(ctx, fx, fy, 5, 1, "rgba(10,10,20,0.35)");
  if (scene.nearby === e.id && scene.player && !playerSeated(scene, scene.playerLayout!)) {
    // In talking range of the chairman: a pulsing ring under their feet.
    const r = 8 + (Math.floor(now / 250) % 2);
    ellipse(ctx, fx, p(a.y), r, 2, C.cyan);
    ellipse(ctx, fx, p(a.y), r - 2, 1, "rgba(10,10,20,0.35)");
  }
  if (scene.selectedEmployee === e.id) {
    ellipse(ctx, fx, p(a.y), 8, 2, C.yellow);
    ellipse(ctx, fx, p(a.y), 6, 1, "rgba(10,10,20,0.35)");
  }
  if (a.leaving) ctx.globalAlpha = 0.85;
  ctx.drawImage(sprite, fx - SPRITE_W / 2, fy - sprite.height + 1 - bob);
  ctx.globalAlpha = 1;
  if (scene.crown === e.id) crown(ctx, fx, fy - sprite.height - 4 - bob);
  if (a.carrying) {
    // A document in hand.
    const hx = fx + (a.facing === 1 ? 5 : -10);
    rect(ctx, hx - 1, fy - 12 - bob, 7, 8, C.outline);
    rect(ctx, hx, fy - 11 - bob, 5, 6, C.white);
    rect(ctx, hx + 1, fy - 9 - bob, 3, 1, C.slate);
    rect(ctx, hx + 1, fy - 7 - bob, 2, 1, C.slate);
  }
}

/** The chairman walking around (when they're away from their desk). */
function drawPlayer(ctx: Ctx, scene: Scene, now: number) {
  const pl = scene.player!;
  const frame: Frame = pl.moving ? (Math.floor(pl.walk) % 2 ? "walkA" : "walkB") : "stand";
  const sprite = characterSprite(CHAIRMAN_LOOK, frame, pl.facing);
  const fx = p(pl.x);
  const fy = p(pl.y);
  const bob = pl.moving && Math.floor(pl.walk) % 2 ? 1 : 0;
  ellipse(ctx, fx, fy, 6, 1, "rgba(10,10,20,0.35)");
  // A small gold marker over the head: "this is you".
  const my = fy - sprite.height - 4 - bob + (Math.floor(now / 400) % 2);
  rect(ctx, fx - 2, my, 5, 2, C.outline);
  rect(ctx, fx - 1, my + 2, 3, 1, C.outline);
  rect(ctx, fx - 1, my, 3, 1, C.yellow);
  px(ctx, fx, my + 1, C.yellow);
  ctx.drawImage(sprite, fx - SPRITE_W / 2, fy - sprite.height + 1 - bob);
}
