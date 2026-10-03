// Движение графа на экране, кроме физики (`graph-physics.ts`): переезд
// узлов к новой раскладке, плавный переход вида, появление, один цикл
// кадров на граф. Чистые функции — с тестами; кадры — через `Frames`.

import type { Point } from "./graph-physics";
import type { View } from "./graph-view";

/** Замедление к концу, без перелёта (кубическое). */
export const ease = (t: number): number => 1 - (1 - t) ** 3;

/**
 * Узлы на доле `e` пути от `from` к `to`: едут только те, что есть в обеих
 * картах, новые — сразу на месте (они проявляются), ушедших нет.
 */
export function glide(from: ReadonlyMap<string, Point>, to: ReadonlyMap<string, Point>, e: number): Map<string, Point> {
  const next = new Map(to);
  // Конец пути — ровно цель: `f + (x - f) * 1` бывает не равно `x` в последнем знаке.
  if (e >= 1) return next;
  for (const [id, [x, y]] of to) {
    const f = from.get(id);
    if (f) next.set(id, [f[0] + (x - f[0]) * e, f[1] + (y - f[1]) * e]);
  }
  return next;
}

/** Есть ли кому ехать: хоть один узел `to` уже нарисован в `from`. */
export const anyMoving = (from: ReadonlyMap<string, Point>, to: ReadonlyMap<string, Point>): boolean => [...to.keys()].some((id) => from.has(id));

/** Вид на доле `e` пути от `from` к `to`. */
export const blendView = (from: View, to: View, e: number): View => ({
  x: from.x + (to.x - from.x) * e,
  y: from.y + (to.y - from.y) * e,
  k: from.k + (to.k - from.k) * e,
});

/**
 * Задержка появления узла, с: от середины рисунка к краям (0 … 0,35 с).
 * `bounds` — `[x0, y0, x1, y1]` раскладки.
 */
export function introDelays(nodes: { id: string; x: number; y: number }[], [x0, y0, x1, y1]: [number, number, number, number]): Map<string, number> {
  const [cx, cy, far] = [(x0 + x1) / 2, (y0 + y1) / 2, Math.max(Math.hypot(x1 - x0, y1 - y0) / 2, 1)];
  return new Map(nodes.map((n) => [n.id, Math.min(Math.hypot(n.x - cx, n.y - cy) / far, 1) * 0.35]));
}

/** Один цикл кадров: новый `run` отменяет прежний. */
export class Frames {
  #frame = 0;

  constructor(
    private readonly request: (cb: FrameRequestCallback) => number = (cb) => requestAnimationFrame(cb),
    private readonly cancel: (id: number) => void = (id) => cancelAnimationFrame(id),
  ) {}

  /** Звать `tick` каждый кадр, пока он возвращает `true`. */
  run(tick: (now: number) => boolean): void {
    this.cancel(this.#frame);
    const step = (now: number) => (this.#frame = tick(now) ? this.request(step) : 0);
    this.#frame = this.request(step);
  }

  /**
   * Переход за `duration` мс: `step(e)` с долей пути `e` (с замедлением),
   * последний кадр — `e = 1`. Метка кадра бывает раньше `start` (начало
   * кадра, в котором позвали) — тогда `e = 0`, а не шаг назад.
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

  /** Идёт ли цикл. */
  get active(): boolean {
    return this.#frame !== 0;
  }
}
