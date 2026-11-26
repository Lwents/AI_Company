import { useEffect, useRef, useState } from "react";
import { useStore } from "@/store";
import { topPerformer } from "@/features/kpi/scoring";
import { buildLayout } from "./layout";
import { createScene, p } from "./engine/scene";
import { applyLiveEvent, tick } from "./engine/simulation";
import { devicePoint, deviceToWorld, dragTo, fitView, hitTest, resetCamera, startDrag, wheelFactor, zoomBy, zoomStep } from "./engine/camera";
import { drawWorld } from "./render/world";
import { compose } from "./render/overlay";
import "./office.css";

/**
 * The animated pixel office. React only mounts the canvas and forwards input;
 * the frame loop runs on its own:  simulation (engine/) → pixel world (render/world)
 * → scaled to screen + crisp text (render/overlay).
 */
export function Office() {
  const { state, onLive, selectedEmployee, setSelectedEmployee, selectedProject } = useStore();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const scene = useRef(createScene()).current;
  const stateRef = useRef(state);

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
        if (now - crownAt > 1000) {
          scene.crown = topPerformer(st);
          crownAt = now;
        }
        if (off.width !== p(layout.W) || off.height !== p(layout.H)) {
          off.width = p(layout.W);
          off.height = p(layout.H);
        }
        fitView(scene, canvas, wrapRef.current!, layout);
        const z = scene.view.k.toFixed(2);
        if (canvas.dataset.zoom !== z) canvas.dataset.zoom = z; // for tests / debugging
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
          canvas().style.cursor = scene.drag?.moved ? "grabbing" : scene.mouse && hitTest(scene, scene.mouse) ? "pointer" : "grab";
        }}
        onPointerUp={(ev) => {
          touches.delete(ev.pointerId);
          if (touches.size > 0) return;
          pinch.dist = 0;
          const g = scene.drag;
          scene.drag = null;
          if (!g || g.moved) return;
          const hit = hitTest(scene, deviceToWorld(scene, devicePoint(canvas(), ev)));
          setSelectedEmployee(hit ? hit.id : null);
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
          if (!touches.size) scene.drag = null;
        }}
      />
      <div className="zoom-ctl">
        <button title="Phóng to" onClick={() => zoomStep(scene, canvas(), true)}>
          +
        </button>
        <button title="Thu nhỏ" onClick={() => zoomStep(scene, canvas(), false)}>
          −
        </button>
        <button title="Vừa khung" onClick={() => resetCamera(scene)}>
          ⤢
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
