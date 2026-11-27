import { useEffect, useRef, useState } from "react";
import { useStore } from "@/store";
import { topPerformer } from "@/features/kpi/scoring";
import { buildLayout } from "./layout";
import { createScene, p } from "./engine/scene";
import { applyLiveEvent, tick } from "./engine/simulation";
import { devicePoint, deviceToWorld, dragTo, fitView, hitTest, hitTestBookshelf, resetCamera, startDrag, wheelFactor, zoomBy, zoomStep } from "./engine/camera";
import { drawWorld } from "./render/world";
import { movePlayer } from "./engine/player";
import { ChatPopup, DPad, InteractMenu, MENU, type MenuAction } from "./Interact";
import { compose } from "./render/overlay";
import "./office.css";

/**
 * The animated pixel office. React only mounts the canvas and forwards input;
 * the frame loop runs on its own:  simulation (engine/) → pixel world (render/world)
 * → scaled to screen + crisp text (render/overlay).
 */
export function Office({ onOpenGuide }: { onOpenGuide?: () => void }) {
  const { state, onLive, selectedEmployee, setSelectedEmployee, selectedProject, watching, setWatching } = useStore();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const scene = useRef(createScene()).current;
  const stateRef = useRef(state);
  // Interaction with the employee next to the chairman: a menu, then an in-office chat.
  const [menu, setMenu] = useState<{ emp: string; index: number } | null>(null);
  const [chatWith, setChatWith] = useState<string | null>(null);
  const menuRef = useRef(menu);
  menuRef.current = menu;
  scene.frozen = !!menu || !!chatWith || !!watching;
  const [touch] = useState(() => typeof matchMedia === "function" && matchMedia("(pointer: coarse)").matches);

  const pick = (action: MenuAction, emp: string) => {
    setMenu(null);
    if (action === "chat") setChatWith(emp);
    else if (action === "profile") setSelectedEmployee(emp);
    else if (action === "screen") setWatching(emp);
  };
  /** E / Enter / the pad's middle button: open the menu, or confirm the highlighted choice. */
  const interact = () => {
    const m = menuRef.current;
    if (m) return pick(MENU[m.index].id, m.emp);
    if (scene.nearby && !chatWith) setMenu({ emp: scene.nearby, index: 0 });
  };
  const navMenu = (dir: "up" | "down") => {
    const m = menuRef.current;
    if (!m) return false;
    setMenu({ ...m, index: (m.index + (dir === "down" ? 1 : MENU.length - 1)) % MENU.length });
    return true;
  };
  const interactRef = useRef(interact);
  interactRef.current = interact;
  const navRef = useRef(navMenu);
  navRef.current = navMenu;

  // Keyboard: arrows / WASD walk, E / Enter / Space interact, Esc closes. Typing in a field is left alone.
  useEffect(() => {
    const KEY: Record<string, keyof typeof scene.keys> = {
      ArrowUp: "up",
      ArrowDown: "down",
      ArrowLeft: "left",
      ArrowRight: "right",
      KeyW: "up",
      KeyS: "down",
      KeyA: "left",
      KeyD: "right",
    };
    const typing = (t: EventTarget | null) => {
      const el = t as HTMLElement | null;
      return !!el && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName));
    };
    const down = (ev: KeyboardEvent) => {
      if (typing(ev.target) || ev.ctrlKey || ev.metaKey || ev.altKey) return;
      if (ev.code === "Escape") {
        setMenu(null);
        setChatWith(null);
        return;
      }
      const dir = KEY[ev.code];
      if (dir) {
        ev.preventDefault();
        if ((dir === "up" || dir === "down") && navRef.current(dir)) return;
        scene.keys[dir] = true;
        return;
      }
      if (ev.code === "KeyE" || ev.code === "Enter" || ev.code === "Space" || ev.code === "NumpadEnter") {
        if (!menuRef.current && !scene.nearby) return;
        ev.preventDefault();
        interactRef.current();
      }
    };
    const up = (ev: KeyboardEvent) => {
      const dir = KEY[ev.code];
      if (dir) scene.keys[dir] = false;
    };
    // Releasing every key when the window loses focus avoids a chairman walking forever.
    const blur = () => Object.assign(scene.keys, { up: false, down: false, left: false, right: false });
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", blur);
    };
  }, [scene]);

  stateRef.current = state;
  scene.selectedEmployee = selectedEmployee;
  scene.selectedProject = selectedProject;

  useEffect(() => {
    const off = onLive((e) => applyLiveEvent(scene, e));
    return () => {
      off();
    };
  }, [onLive, scene]);

  useEffect(() => {
    const canvas = canvasRef.current!;
    const ctx = canvas.getContext("2d")!;
    const off = document.createElement("canvas");
    const octx = off.getContext("2d")!;
    let raf = 0;
    let last = performance.now();
    let crownAt = 0;
    void document.fonts?.load(`16px VT323`);

    const frame = (now: number) => {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      const st = stateRef.current;
      if (st) {
        const layout = buildLayout(st.employees.filter((e) => e.enabled));
        scene.bookshelves = layout.bookshelves;
        if (now - crownAt > 1000) {
          scene.crown = topPerformer(st);
          crownAt = now;
        }
        scene.playerLayout = layout;
        movePlayer(scene, layout, dt);
        if (off.width !== p(layout.W) || off.height !== p(layout.H)) {
          off.width = p(layout.W);
          off.height = p(layout.H);
        }
        fitView(scene, canvas, wrapRef.current!, layout);
        const z = scene.view.k.toFixed(2);
        if (canvas.dataset.zoom !== z) canvas.dataset.zoom = z; // for tests / debugging
        const pl = scene.player;
        if (pl) {
          const pos = `${Math.round(pl.x)},${Math.round(pl.y)}`;
          if (canvas.dataset.player !== pos) canvas.dataset.player = pos;
          if (canvas.dataset.nearby !== (scene.nearby ?? "")) canvas.dataset.nearby = scene.nearby ?? "";
        }
        tick(scene, st, layout, dt, now);
        drawWorld(octx, scene, st, layout, now);
        compose(ctx, off, scene, st, layout, now);
      }
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);

    // Native, non-passive listener: React's onWheel is passive and can't stop the page
    // from scrolling or the browser from zooming the whole page (touchpad pinch / Ctrl+wheel).
    const onWheel = (ev: WheelEvent) => {
      ev.preventDefault();
      zoomBy(scene, canvas, devicePoint(canvas, ev), wheelFactor(ev));
    };
    canvas.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      cancelAnimationFrame(raf);
      canvas.removeEventListener("wheel", onWheel);
    };
  }, [scene]);

  const canvas = () => canvasRef.current!;
  // Active pointers (fingers) for pinch-zoom; kept across renders without causing any.
  const [touches] = useState(() => new Map<number, { x: number; y: number }>());
  const [pinch] = useState(() => ({ dist: 0 }));

  return (
    <div className="office" ref={wrapRef}>
      <canvas
        ref={canvasRef}
        // Pointer events: mouse, pen and touch alike. One finger drags, two fingers pinch-zoom.
        onPointerDown={(ev) => {
          canvas().setPointerCapture(ev.pointerId);
          touches.set(ev.pointerId, devicePoint(canvas(), ev));
          if (touches.size === 1) {
            startDrag(scene, canvas(), devicePoint(canvas(), ev));
          } else if (touches.size === 2) {
            scene.drag = null; // a pinch is never a click
            pinch.dist = spread(touches);
          }
        }}
        onPointerMove={(ev) => {
          const d = devicePoint(canvas(), ev);
          if (touches.has(ev.pointerId)) touches.set(ev.pointerId, d);
          if (touches.size >= 2) {
            const dist = spread(touches);
            if (pinch.dist > 0 && dist > 0) zoomBy(scene, canvas(), middle(touches), dist / pinch.dist);
            pinch.dist = dist;
            return;
          }
          if (ev.pointerType === "mouse") scene.mouse = deviceToWorld(scene, d);
          if (touches.size === 1) dragTo(scene, d);
          const empHit = scene.mouse ? hitTest(scene, scene.mouse) : null;
          const shelfHit = !empHit && scene.mouse ? hitTestBookshelf(scene, scene.mouse) : null;
          scene.hoveredBookshelf = shelfHit ? shelfHit.id : null;
          canvas().style.cursor = scene.drag?.moved ? "grabbing" : empHit || shelfHit ? "pointer" : "grab";
        }}
        onPointerUp={(ev) => {
          touches.delete(ev.pointerId);
          if (touches.size > 0) return;
          pinch.dist = 0;
          const g = scene.drag;
          scene.drag = null;
          if (!g || g.moved) return;
          const wp = deviceToWorld(scene, devicePoint(canvas(), ev));
          const hit = hitTest(scene, wp);
          if (hit) {
            setSelectedEmployee(hit.id);
            return;
          }
          const shelf = hitTestBookshelf(scene, wp);
          if (shelf) {
            onOpenGuide?.();
            return;
          }
          setSelectedEmployee(null);
        }}
        onPointerCancel={(ev) => {
          touches.delete(ev.pointerId);
          pinch.dist = 0;
          scene.drag = null;
        }}
        onDoubleClick={() => resetCamera(scene)}
        onPointerLeave={(ev) => {
          if (ev.pointerType !== "mouse") return;
          scene.mouse = null;
          scene.hoveredBookshelf = null;
          if (!touches.size) scene.drag = null;
        }}
      />
      {menu && <InteractMenu empId={menu.emp} index={menu.index} onPick={(a) => pick(a, menu.emp)} />}
      {chatWith && <ChatPopup empId={chatWith} onClose={() => setChatWith(null)} />}
      {touch && !chatWith && <DPad keys={scene.keys} onAction={() => interactRef.current()} onNav={(d) => navRef.current(d)} />}
      <div className="zoom-ctl">
        <button title="Phóng to" aria-label="Phóng to" onClick={() => zoomStep(scene, canvas(), true)}>
          <svg viewBox="0 0 10 10" aria-hidden="true">
            <path d="M4 1h2v3h3v2H6v3H4V6H1V4h3z" />
          </svg>
        </button>
        <button title="Thu nhỏ" aria-label="Thu nhỏ" onClick={() => zoomStep(scene, canvas(), false)}>
          <svg viewBox="0 0 10 10" aria-hidden="true">
            <path d="M1 4h8v2H1z" />
          </svg>
        </button>
        <button title="Vừa khung" aria-label="Vừa khung" onClick={() => resetCamera(scene)}>
          <svg viewBox="0 0 10 10" aria-hidden="true">
            <path d="M0 0h4v2H2v2H0zM6 0h4v4H8V2H6zM0 6h2v2h2v2H0zM8 6h2v4H6V8h2z" />
          </svg>
        </button>
      </div>
    </div>
  );
}

type Pt = { x: number; y: number };
const pts = (m: Map<number, Pt>) => [...m.values()].slice(0, 2);
/** Distance between the first two fingers. */
function spread(m: Map<number, Pt>): number {
  const [a, b] = pts(m);
  return a && b ? Math.hypot(a.x - b.x, a.y - b.y) : 0;
}
/** Point between the first two fingers: the pinch zooms around it. */
function middle(m: Map<number, Pt>): Pt {
  const [a, b] = pts(m);
  return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
}
