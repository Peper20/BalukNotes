import { describe, expect, it } from "vitest";
import { fitView, matches, zoomAt } from "./graph-view";

describe("graph: search", () => {
  it("by path and title, case-insensitive", () => {
    expect(matches("сеть/a", "Сеть/A")).toBe(true);
    expect(matches("ИНТЕГРАЛ", "Мат/C", "Интеграл Эйлера")).toBe(true);
    expect(matches("  ", "Сеть/A")).toBe(false);
  });
});

describe("graph: zoom", () => {
  it("the point under the pointer stays in place; a zoom limit", () => {
    const v = zoomAt({ x: 10, y: 20, k: 1 }, 2, 100, 50);
    // the world under (100, 50) before: (90, 30); after - the same
    expect([(100 - v.x) / v.k, (50 - v.y) / v.k]).toEqual([90, 30]);
    expect(zoomAt(v, 1000, 0, 0).k).toBe(6);
  });

  it("fit: centered, a small graph is not blown up", () => {
    const v = fitView([0, 0, 100, 50], 400, 300, 0);
    expect(v.k).toBe(1.6);
    expect([v.x + 50 * v.k, v.y + 25 * v.k]).toEqual([200, 150]);
    expect(fitView([0, 0, 1000, 100], 500, 300, 0).k).toBe(0.5);
  });
});
