import { expect, it } from "vitest";
import { clampActive, closeAt, cycle, openAfter } from "./tabs";

const list = (...urls: string[]) => urls.map((url) => ({ url }));

it("новая вкладка — сразу после активной", () => {
  expect(openAfter({ tabs: list("/a", "/b", "/c"), active: 0 }, "/x")).toEqual({ tabs: list("/a", "/x", "/b", "/c"), active: 1 });
  expect(openAfter({ tabs: list("/a"), active: 0 }, "/x")).toEqual({ tabs: list("/a", "/x"), active: 1 });
});

it("закрыть вкладку: активная та же, закрытая активная — соседняя", () => {
  const s = { tabs: list("/a", "/b", "/c"), active: 1 };
  expect(closeAt(s, 0)).toEqual({ tabs: list("/b", "/c"), active: 0, wasActive: false });
  expect(closeAt(s, 2)).toEqual({ tabs: list("/a", "/b"), active: 1, wasActive: false });
  expect(closeAt(s, 1)).toEqual({ tabs: list("/a", "/c"), active: 1, wasActive: true });
  expect(closeAt({ tabs: list("/a", "/b"), active: 1 }, 1)).toEqual({ tabs: list("/a"), active: 0, wasActive: true });
  expect(closeAt({ tabs: list("/a"), active: 0 }, 0)).toBeNull();
  expect(closeAt(s, 5)).toBeNull();
});

it("по кругу и в пределах", () => {
  expect(cycle(2, 3, 1)).toBe(0);
  expect(cycle(0, 3, -1)).toBe(2);
  expect(clampActive(7, 3)).toBe(2);
  expect(clampActive(-1, 3)).toBe(0);
});
