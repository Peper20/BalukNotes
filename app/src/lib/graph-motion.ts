// Graph motion on screen besides physics (`graph-physics.ts`): nodes moving
// to a new layout, a smooth view transition, appearing, one frame loop per
// graph. Pure functions have tests; frames go through `Frames`.

import type { Point } from "./graph-physics";
import type { View } from "./graph-view";

/** Easing out towards the end, no overshoot (cubic). */
export const ease = (t: number): number => 1 - (1 - t) ** 3;

/**
 * Nodes at the part `e` of the way from `from` to `to`: only those in both
 * maps move, new ones are in place at once (they fade in), gone ones are dropped.
 */
export function glide(from: ReadonlyMap<string, Point>, to: ReadonlyMap<string, Point>, e: number): Map<string, Point> {
  const next = new Map(to);
  // The end of the way is exactly the target: `f + (x - f) * 1` may differ from `x` in the last digit.
  if (e >= 1) return next;
  for (const [id, [x, y]] of to) {
    const f = from.get(id);
    if (f) next.set(id, [f[0] + (x - f[0]) * e, f[1] + (y - f[1]) * e]);
  }
  return next;
}

/** Whether anyone moves: at least one node of `to` is already drawn in `from`. */
export const anyMoving = (from: ReadonlyMap<string, Point>, to: ReadonlyMap<string, Point>): boolean => [...to.keys()].some((id) => from.has(id));

/** The view at the part `e` of the way from `from` to `to`. */
export const blendView = (from: View, to: View, e: number): View => ({
  x: from.x + (to.x - from.x) * e,
  y: from.y + (to.y - from.y) * e,
  k: from.k + (to.k - from.k) * e,
});

/**
 * Appearing delay of a node, s: from the middle of the figure to the edges
 * (0 ... 0.35 s). `bounds` is the layout's `[x0, y0, x1, y1]`.
 */
export function introDelays(nodes: { id: string; x: number; y: number }[], [x0, y0, x1, y1]: [number, number, number, number]): Map<string, number> {
  const [cx, cy, far] = [(x0 + x1) / 2, (y0 + y1) / 2, Math.max(Math.hypot(x1 - x0, y1 - y0) / 2, 1)];
  return new Map(nodes.map((n) => [n.id, Math.min(Math.hypot(n.x - cx, n.y - cy) / far, 1) * 0.35]));
}

/** One frame loop: a new `run` cancels the previous one. */
export class Frames {
  #frame = 0;

  constructor(
    private readonly request: (cb: FrameRequestCallback) => number = (cb) => requestAnimationFrame(cb),
    private readonly cancel: (id: number) => void = (id) => cancelAnimationFrame(id),
  ) {}

  /** Calls `tick` every frame while it returns `true`. */
  run(tick: (now: number) => boolean): void {
    this.cancel(this.#frame);
    const step = (now: number) => (this.#frame = tick(now) ? this.request(step) : 0);
    this.#frame = this.request(step);
  }

  /**
   * A transition over `duration` ms: `step(e)` with the part of the way `e`
   * (eased), the last frame is `e = 1`. A frame timestamp may be earlier than
   * `start` (the start of the frame it was called in) - then `e = 0`, not a
   * step back.
   */
  tween(duration: number, step: (e: number) => void, start = performance.now()): void {
    this.run((now) => {
      const t = Math.min(Math.max((now - start) / duration, 0), 1);
      step(ease(t));
      return t < 1;
    });
  }

  stop(): void {
    this.cancel(this.#frame);
    this.#frame = 0;
  }

  /** Whether the loop runs. */
  get active(): boolean {
    return this.#frame !== 0;
  }
}
