// Graph gestures without the DOM: pointers and what they do - panning the
// background, two fingers (zoom and pan), a dragged node, a click.
// Coordinates are screen ones (from the figure corner); what to do with a
// node (physics, open it) is up to the component (`Graph.svelte`).

import type { Point } from "./graph-physics";
import { zoomAt, type View } from "./graph-view";

/** Pointer shift (px) after which pressing a node is a drag, not a click. */
export const DRAG_START = 4;

export type Gesture =
  | { kind: "pan"; start: Point; view: View }
  | { kind: "node"; id: string; start: Point; grab: Point; far: boolean }
  | { kind: "pinch"; dist: number; mid: Point; view: View };

/** What to do after the pointer moved. */
export type Move = { kind: "none" } | { kind: "view"; view: View; pan: boolean } | { kind: "drag"; id: string; grab: Point; first: boolean };

/** What to do when the pointer is released. */
export type Up =
  | { kind: "none" }
  /** Someone else still holds: the gesture goes on. */
  | { kind: "continue" }
  /** All released; `gesture` is what the gesture was. */
  | { kind: "end"; gesture: Gesture | null };

const mid = (a: Point, b: Point): Point => [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
const dist = (a: Point, b: Point) => Math.hypot(a[0] - b[0], a[1] - b[1]);

/** The view with two fingers `a`, `b`: zoom at the starting middle and pan after it. */
export function pinchView(g: Extract<Gesture, { kind: "pinch" }>, a: Point, b: Point): View {
  const m = mid(a, b);
  const v = zoomAt(g.view, dist(a, b) / g.dist, g.mid[0], g.mid[1]);
  return { ...v, x: v.x + m[0] - g.mid[0], y: v.y + m[1] - g.mid[1] };
}

/** The view when panning the background to the point `p`. */
export const panView = (g: Extract<Gesture, { kind: "pan" }>, p: Point): View => ({
  ...g.view,
  x: g.view.x + p[0] - g.start[0],
  y: g.view.y + p[1] - g.start[1],
});

/** Pointers on the figure and the current gesture. */
export class Pointers {
  readonly at = new Map<number, Point>();
  gesture: Gesture | null = null;

  get size(): number {
    return this.at.size;
  }

  /**
   * Pointer `id` pressed at the point `p`: on a node (`node` is its id and
   * where it was grabbed relative to the center) or on the background. A
   * second finger is zoom (`"pinch"`: time to release the dragged node); a
   * third is not a gesture.
   */
  down(id: number, p: Point, view: View, node: { id: string; grab: Point } | null): Gesture["kind"] | null {
    this.at.set(id, p);
    if (this.at.size === 2) {
      const [a, b] = [...this.at.values()] as [Point, Point];
      this.gesture = { kind: "pinch", dist: Math.max(dist(a, b), 1), mid: mid(a, b), view };
      return "pinch";
    }
    if (this.at.size > 2) return null;
    this.gesture = node ? { kind: "node", id: node.id, start: p, grab: node.grab, far: false } : { kind: "pan", start: p, view };
    return this.gesture.kind;
  }

  /** Pointer `id` moved to `p`. `canDrag`: whether the graph still has such a node. */
  move(id: number, p: Point, canDrag: (node: string) => boolean = () => true): Move {
    const g = this.gesture;
    if (!this.at.has(id) || !g) return { kind: "none" };
    this.at.set(id, p);
    if (g.kind === "pinch") {
      if (this.at.size !== 2) return { kind: "none" };
      const [a, b] = [...this.at.values()] as [Point, Point];
      return { kind: "view", view: pinchView(g, a, b), pan: false };
    }
    if (g.kind === "pan") return { kind: "view", view: panView(g, p), pan: true };
    if (!g.far && dist(p, g.start) < DRAG_START) return { kind: "none" };
    if (!canDrag(g.id)) return { kind: "none" };
    const first = !g.far;
    g.far = true;
    return { kind: "drag", id: g.id, grab: g.grab, first };
  }

  /** Pointer `id` released (or lost). `view` is the view now. */
  up(id: number, view: View): Up {
    if (!this.at.delete(id)) return { kind: "none" };
    const g = this.gesture;
    if (this.at.size === 1 && g?.kind === "pinch") {
      // One finger left: pan from it from now on.
      this.gesture = { kind: "pan", start: [...this.at.values()][0]!, view };
      return { kind: "continue" };
    }
    if (this.at.size > 0) return { kind: "continue" };
    this.gesture = null;
    return { kind: "end", gesture: g };
  }
}
