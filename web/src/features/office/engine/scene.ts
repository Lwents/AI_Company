import type { Employee } from "@/api/types";
import type { InteractiveBox, P } from "../layout";

/**
 * Units:
 *  - world units: what the simulation and layout use;
 *  - office pixels (world / U): the low-res pixel-art canvas;
 *  - device pixels: the on-screen canvas (office pixels × zoom k).
 */
export const U = 2;
/** World → office pixel. */
export const p = (v: number) => Math.round(v / U);

export const FONT = "VT323, ui-monospace, monospace";
/** Walking speed, world units per second. */
export const SPEED = 110;
/** Seated people are drawn this many office pixels higher, so the upper body shows above the desk. */
export const SEAT = 5;

export type Task =
  | { kind: "deliver"; to: string; text: string; stage: "go" | "talk" | "back"; until?: number }
  | { kind: "coffee"; stage: "go" | "sip" | "back"; until?: number };

/** One employee walking around the office. */
export interface Actor {
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

/** World → device pixels: screen = o + world * scale, where scale = k / U. */
export interface View {
  scale: number;
  ox: number;
  oy: number;
  k: number;
  /** Zoom that fits the whole office in the canvas (the zoom-out limit). */
  fitK: number;
}

/** Camera: auto-fit, or manual zoom centred on (cx, cy) in office pixels. */
export interface Camera {
  manual: boolean;
  zoom: number;
  cx: number;
  cy: number;
}

/** Mutable, per-mount office state shared by the simulation, renderer and input handlers. */
export interface Scene {
  actors: Map<string, Actor>;
  /** False until the first tick: on first load everyone is already seated. */
  initialized: boolean;
  meeting: { participants: string[]; topic: string } | null;
  /** Employee wearing the crown (best KPI). */
  crown: string | null;
  selectedEmployee: string | null;
  selectedProject: string | null;
  /** Clickable bookshelves from the current layout. */
  bookshelves: InteractiveBox[];
  /** Currently hovered bookshelf id, if any. */
  hoveredBookshelf: string | null;
  /** Mouse position in world units, null when outside. */
  mouse: P | null;
  view: View;
  cam: Camera;
  drag: { x: number; y: number; cx: number; cy: number; moved: boolean } | null;
}

export function createScene(): Scene {
  return {
    actors: new Map(),
    initialized: false,
    meeting: null,
    crown: null,
    selectedEmployee: null,
    selectedProject: null,
    bookshelves: [],
    hoveredBookshelf: null,
    mouse: null,
    view: { scale: 1, ox: 0, oy: 0, k: 1, fitK: 1 },
    cam: { manual: false, zoom: 2, cx: 0, cy: 0 },
    drag: null,
  };
}
