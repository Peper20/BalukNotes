import { describe, expect, it } from "vitest";
import { fitView, matches, zoomAt } from "./graph-view";

describe("граф: поиск", () => {
  it("по пути и названию, без учёта регистра", () => {
    expect(matches("сеть/a", "Сеть/A")).toBe(true);
    expect(matches("ИНТЕГРАЛ", "Мат/C", "Интеграл Эйлера")).toBe(true);
    expect(matches("  ", "Сеть/A")).toBe(false);
  });
});

describe("граф: масштаб", () => {
  it("точка под указателем остаётся на месте; предел увеличения", () => {
    const v = zoomAt({ x: 10, y: 20, k: 1 }, 2, 100, 50);
    // мир под (100, 50) до: (90, 30); после — тот же
    expect([(100 - v.x) / v.k, (50 - v.y) / v.k]).toEqual([90, 30]);
    expect(zoomAt(v, 1000, 0, 0).k).toBe(6);
  });

  it("вписать: по центру, мелкий граф не раздувается", () => {
    const v = fitView([0, 0, 100, 50], 400, 300, 0);
    expect(v.k).toBe(1.6);
    expect([v.x + 50 * v.k, v.y + 25 * v.k]).toEqual([200, 150]);
    expect(fitView([0, 0, 1000, 100], 500, 300, 0).k).toBe(0.5);
  });
});
