import { useEffect, useRef } from "react";
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

  return (
    <div className="office" ref={wrapRef}>
      <canvas
        ref={canvasRef}
        onMouseDown={(ev) => startDrag(scene, canvas(), devicePoint(canvas(), ev))}
        onMouseMove={(ev) => {
          const d = devicePoint(canvas(), ev);
          scene.mouse = deviceToWorld(scene, d);
          if (ev.buttons & 1) dragTo(scene, d);
          canvas().style.cursor = scene.drag?.moved ? "grabbing" : hitTest(scene, scene.mouse) ? "pointer" : "grab";
        }}
        onMouseUp={(ev) => {
          const g = scene.drag;
          scene.drag = null;
          if (g?.moved) return;
          const hit = hitTest(scene, deviceToWorld(scene, devicePoint(canvas(), ev)));
          setSelectedEmployee(hit ? hit.id : null);
        }}
        onDoubleClick={() => resetCamera(scene)}
        onMouseLeave={() => {
          scene.mouse = null;
          scene.drag = null;
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
