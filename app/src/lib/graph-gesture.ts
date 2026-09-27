// Жесты графа без DOM: указатели и что они делают — сдвиг фона, два пальца
// (масштаб и сдвиг), протянутый узел, щелчок. Координаты — экранные
// (от угла рисунка); что делать с узлом (физика, открыть) — решает
// компонент (`Graph.svelte`).

import type { Point } from "./graph-physics";
import { zoomAt, type View } from "./graph-view";

/** Сдвиг указателя (px), после которого нажатие на узел — перетаскивание, а не щелчок. */
export const DRAG_START = 4;

export type Gesture =
  | { kind: "pan"; start: Point; view: View }
  | { kind: "node"; id: string; start: Point; grab: Point; far: boolean }
  | { kind: "pinch"; dist: number; mid: Point; view: View };

/** Что сделать после движения указателя. */
export type Move = { kind: "none" } | { kind: "view"; view: View; pan: boolean } | { kind: "drag"; id: string; grab: Point; first: boolean };

/** Что сделать, когда указатель отпустили. */
export type Up =
  | { kind: "none" }
  /** Ещё кто-то держит: жест продолжается. */
  | { kind: "continue" }
  /** Все отпустили; `gesture` — каким был жест. */
  | { kind: "end"; gesture: Gesture | null };

const mid = (a: Point, b: Point): Point => [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
const dist = (a: Point, b: Point) => Math.hypot(a[0] - b[0], a[1] - b[1]);

/** Вид при двух пальцах `a`, `b`: масштаб у начальной середины и сдвиг вслед за ней. */
export function pinchView(g: Extract<Gesture, { kind: "pinch" }>, a: Point, b: Point): View {
  const m = mid(a, b);
  const v = zoomAt(g.view, dist(a, b) / g.dist, g.mid[0], g.mid[1]);
  return { ...v, x: v.x + m[0] - g.mid[0], y: v.y + m[1] - g.mid[1] };
}

/** Вид при сдвиге фона до точки `p`. */
export const panView = (g: Extract<Gesture, { kind: "pan" }>, p: Point): View => ({
  ...g.view,
  x: g.view.x + p[0] - g.start[0],
  y: g.view.y + p[1] - g.start[1],
});

/** Указатели на рисунке и текущий жест. */
export class Pointers {
  readonly at = new Map<number, Point>();
  gesture: Gesture | null = null;

  get size(): number {
    return this.at.size;
  }

  /**
   * Нажали указатель `id` в точке `p`: на узле (`node` — его id и где за
   * него взялись относительно центра) или на фоне. Второй палец — масштаб
   * (`"pinch"`: протянутый узел пора отпустить); третий — не жест.
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

  /** Указатель `id` сдвинулся в `p`. `canDrag` — есть ли ещё такой узел в графе. */
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

  /** Отпустили указатель `id` (или он пропал). `view` — вид сейчас. */
  up(id: number, view: View): Up {
    if (!this.at.delete(id)) return { kind: "none" };
    const g = this.gesture;
    if (this.at.size === 1 && g?.kind === "pinch") {
      // Остался один палец — дальше сдвиг от него.
      this.gesture = { kind: "pan", start: [...this.at.values()][0]!, view };
      return { kind: "continue" };
    }
    if (this.at.size > 0) return { kind: "continue" };
    this.gesture = null;
    return { kind: "end", gesture: g };
  }
}
