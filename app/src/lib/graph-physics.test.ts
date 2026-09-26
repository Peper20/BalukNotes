import { describe, expect, it } from "vitest";
import { extentOf, Physics, type PhysicsNode, type Rect } from "./graph-physics";

// Цепочка a — b — c — d по горизонтали и e без связей в стороне.
const node = (x: number, y: number, name = "узел"): PhysicsNode => ({ x, y, extent: extentOf(6, name, 11, 2) });
const nodes = () => [node(0, 0), node(70, 0), node(140, 0), node(210, 0), node(100, 120)];
const edges: [number, number][] = [
  [0, 1],
  [1, 2],
  [2, 3],
];

/** Шагать, пока не осядет (не больше `limit` шагов); возвращает число шагов. */
function settle(p: Physics, limit = 2000): number {
  for (let i = 1; i <= limit; i++) if (p.settled(p.step())) return i;
  return limit;
}

describe("физика графа", () => {
  it("раскладка ядра — покой: нетронутый граф не шевелится", () => {
    const p = new Physics(nodes(), edges);
    const before = p.pos.slice();
    expect(p.settled(p.step())).toBe(true);
    expect([...p.pos]).toEqual([...before]);
  });

  it("соседи тянутся за узлом, дальние — слабее, несвязанный стоит", () => {
    const p = new Physics(nodes(), edges);
    for (let i = 0; i < 60; i++) {
      p.drag(0, -80, 40);
      p.step();
    }
    const dx = (i: number) => p.at(i)[0] - nodes()[i]!.x;
    expect(dx(1)).toBeLessThan(-10);
    expect(dx(2)).toBeLessThan(0);
    expect(Math.abs(dx(2))).toBeLessThan(Math.abs(dx(1)));
    expect(Math.abs(dx(3))).toBeLessThan(Math.abs(dx(2)));
    expect(p.at(4)).toEqual([100, 120]);
  });

  it("отпустили — узел стоит, соседи плавно отходят немного назад и встают", () => {
    const p = new Physics(nodes(), edges);
    for (let i = 0; i < 60; i++) {
      p.drag(3, 300, 60);
      p.step();
    }
    const [x2] = p.at(2);
    expect(x2).toBeGreaterThan(160);
    const drop = p.at(3);
    p.release();
    const xs: number[] = [];
    let steps = 0;
    for (; steps < 300; steps++) {
      const f = p.step();
      xs.push(p.at(2)[0]);
      if (p.settled(f)) break;
    }
    expect(steps).toBeLessThan(120); // встали за пару секунд
    const end = xs.at(-1)!;
    // прошли около четверти пути к прежнему месту (140), без перелёта
    expect((x2 - end) / (x2 - 140)).toBeGreaterThan(0.1);
    expect((x2 - end) / (x2 - 140)).toBeLessThan(0.4);
    expect(Math.min(...xs)).toBeGreaterThan(end - 0.5);
    expect(p.at(3)).toEqual(drop); // брошенный не качнулся
  });

  it("узел держат на месте — соседи подходят без раскачки", () => {
    const p = new Physics(nodes(), edges);
    const xs: number[] = [];
    for (let i = 0; i < 200; i++) {
      p.drag(0, -Math.min(i, 20) * 8, 0); // тянут влево и держат
      p.step();
      xs.push(p.at(1)[0]);
    }
    const end = xs.at(-1)!;
    // сосед едет влево и не проскакивает конечное место
    expect(Math.min(...xs.slice(20))).toBeGreaterThan(end - 0.5);
  });

  it("детерминированно: одинаковые действия — одинаковые координаты", () => {
    const run = () => {
      const p = new Physics(nodes(), edges);
      for (let i = 0; i < 40; i++) {
        p.drag(1, 70 + i * 3, i * 2);
        p.step();
      }
      p.release();
      settle(p);
      return [...p.pos];
    };
    expect(run()).toEqual(run());
  });

  it("наехавшие узлы расталкиваются", () => {
    const p = new Physics(nodes(), edges);
    p.drag(4, 140, 10); // на c
    for (let i = 0; i < 100; i++) p.step();
    const [cx, cy] = p.at(2);
    const box = extentOf(6, "узел", 11, 2);
    const apart = Math.abs(cx - 140) >= 2 * box.half - 1 || Math.abs(cy - 10) >= box.top + box.bottom - 1;
    expect(apart).toBe(true);
  });

  it("размеры по экрану крупнее — покой всё равно покой, а наехавшие раздвигаются по ним", () => {
    const p = new Physics(nodes(), edges);
    const big = extentOf(6, "очень длинная подпись узла", 14, 2);
    p.setExtents(nodes().map(() => big));
    const before = p.pos.slice();
    expect(p.settled(p.step())).toBe(true);
    expect([...p.pos]).toEqual([...before]);
    p.drag(4, 100, 0); // между b и c
    for (let i = 0; i < 100; i++) p.step();
    expect(Math.abs(p.at(1)[0] - 100) > 12.76 * 2 || Math.abs(p.at(2)[0] - 100) > 12.76 * 2).toBe(true);
  });

  it("рамка: утащить за край нельзя ни узел, ни соседей", () => {
    const frame: Rect = [-20, -20, 230, 150];
    const p = new Physics(nodes(), edges, frame);
    const inside = () => {
      for (let i = 0; i < p.n; i++) {
        const [x, y] = p.at(i);
        const e = extentOf(6, "узел", 11, 2);
        expect(x - e.half).toBeGreaterThanOrEqual(frame[0] - 1e-9);
        expect(x + e.half).toBeLessThanOrEqual(frame[2] + 1e-9);
        expect(y - e.top).toBeGreaterThanOrEqual(frame[1] - 1e-9);
        expect(y + e.bottom).toBeLessThanOrEqual(frame[3] + 1e-9);
      }
    };
    for (let i = 0; i < 200; i++) {
      p.drag(0, -1000 - i, -1000);
      p.step();
      inside();
    }
    expect(p.at(1)[0]).toBeLessThan(68); // сосед всё же подтянулся (узел упёрся в угол рамки)
    p.release();
    settle(p);
    inside();
  });

  it("у края рамки наехавшие раздвигаются вдоль края", () => {
    // Узел у верхнего края, под него подсунули другой: вверх некуда — в сторону.
    const p = new Physics([node(0, 0), node(100, 0)], [], [-200, -6, 300, 200]);
    for (let i = 0; i < 100; i++) {
      p.drag(1, 0, 12);
      p.step();
    }
    const e = extentOf(6, "узел", 11, 2);
    expect(p.at(0)[1]).toBeCloseTo(0, 6);
    expect(Math.abs(p.at(0)[0])).toBeGreaterThanOrEqual(2 * e.half - 0.5);
  });

  it("рамка не сдвигает узел, стоящий дома за ней (крупная подпись)", () => {
    const p = new Physics(nodes(), edges, [0, 0, 100, 100]);
    expect(p.settled(p.step())).toBe(true);
    expect(p.at(3)).toEqual([210, 0]);
  });
});
