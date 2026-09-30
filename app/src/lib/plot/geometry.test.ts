import { describe, expect, it } from "vitest";
import { clipRuns, faces, formatNumber, mixHex, niceStep, sample, ticks, turn, view } from "./geometry";

describe("оси", () => {
  it("шаг делений — 1, 2, 5 × 10^k", () => {
    expect(niceStep(10)).toBe(2);
    expect(niceStep(6)).toBe(1);
    expect(niceStep(0.3)).toBeCloseTo(0.05);
    expect(niceStep(250)).toBe(50);
  });

  it("подписи чисел", () => {
    expect(formatNumber(-2, 1)).toBe("−2");
    expect(formatNumber(0.5, 0.1)).toBe("0.5");
    expect(formatNumber(1, 0.04)).toBe("1.00");
    expect(formatNumber(1, 0.04, true)).toBe("1");
    expect(formatNumber(-1e-12, 0.5)).toBe("0.0");
  });

  it("деления в диапазоне", () => {
    expect(ticks(-5, 5, 2)).toEqual([-4, -2, 0, 2, 4]);
    expect(ticks(0, 1, 0.5)).toEqual([0, 0.5, 1]);
  });
});

describe("кривая", () => {
  it("разрыв на асимптоте и вне области определения", () => {
    const tan = clipRuns(sample(Math.tan, -3, 3, 120), -4, 4);
    expect(tan.length).toBe(3);
    const sqrt = clipRuns(sample((x) => (x < 0 ? null : Math.sqrt(x)), -1, 4, 50), -1, 3);
    expect(sqrt.length).toBe(1);
    expect(sqrt[0]![0]![0]).toBeGreaterThanOrEqual(0);
  });

  it("обрезка по краю полосы", () => {
    const runs = clipRuns(
      [
        [0, 0],
        [1, 2],
        [2, 0],
      ],
      -1,
      1,
    );
    expect(runs).toEqual([
      [
        [0, 0],
        [0.5, 1],
      ],
      [
        [1.5, 1],
        [2, 0],
      ],
    ]);
  });
});

describe("поверхность", () => {
  it("вид сверху без поворота: x вправо, y вверх", () => {
    const [u, v] = view([1, 0, 0], 0, Math.PI / 2);
    expect(u).toBeCloseTo(1);
    expect(v).toBeCloseTo(0);
    expect(view([0, 1, 0], 0, Math.PI / 2)[1]).toBeCloseTo(1);
  });

  it("грани — от дальних к ближним, узлы вне диапазона пропускаются", () => {
    const grid = [
      [0, 0, 0],
      [0, 5, 0],
      [0, 0, 0],
    ];
    expect(faces(grid, -1, 1, 0.3, 0.5)).toEqual([]);
    const flat = faces(
      grid.map((r) => r.map(() => 0)),
      -1,
      1,
      -0.5,
      0.5,
    );
    expect(flat.length).toBe(4);
    for (let i = 1; i < flat.length; i++) expect(flat[i]!.depth).toBeGreaterThanOrEqual(flat[i - 1]!.depth);
  });

  it("смесь цветов", () => {
    expect(mixHex("#000000", "#ffffff", 0.5)).toBe("rgba(128, 128, 128, 1.000)");
    expect(mixHex("#ff000080", "#ff0000", 0)).toBe("rgba(255, 0, 0, 0.502)");
  });
});

describe("поворот", () => {
  const near: [number, number, number] = [0, -1, 0]; // ближняя сторона при θ = 0
  it("вправо — ближняя сторона едет вправо, вниз — вниз", () => {
    const [th, ph] = [0, 0.5];
    const [th2, ph2] = turn(th, ph, 0.1, 0.1);
    expect(view(near, th2, ph2)[0]).toBeGreaterThan(view(near, th, ph)[0]);
    expect(view(near, th2, ph2)[1]).toBeLessThan(view(near, th, ph)[1]);
  });
  it("наклон — в пределах", () => {
    expect(turn(0, 0, 0, 10)[1]).toBeCloseTo((89 * Math.PI) / 180);
    expect(turn(0, 0, 0, -10)[1]).toBeCloseTo((-10 * Math.PI) / 180);
  });
});
