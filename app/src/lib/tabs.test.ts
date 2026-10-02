import { expect, it } from "vitest";
import { clampActive, closeAt, cycle, dropTabs, openAfter, openBehind } from "./tabs";

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

it("убрать вкладки удалённой заметки", () => {
  const isA = (t: { url: string }) => t.url.startsWith("/a");
  // Активная осталась — она же активная.
  expect(dropTabs({ tabs: list("/a", "/b", "/a#x", "/c"), active: 3 }, isA, "/")).toEqual({ tabs: list("/b", "/c"), active: 1, activeDropped: false });
  // Убрали активную — ближайшая слева; слева никого — справа.
  expect(dropTabs({ tabs: list("/b", "/a", "/c"), active: 1 }, isA, "/")).toEqual({ tabs: list("/b", "/c"), active: 0, activeDropped: true });
  expect(dropTabs({ tabs: list("/a", "/c"), active: 0 }, isA, "/")).toEqual({ tabs: list("/c"), active: 0, activeDropped: true });
  // Не осталось ни одной — одна новая.
  expect(dropTabs({ tabs: list("/a", "/a#y"), active: 1 }, isA, "/home")).toEqual({ tabs: list("/home"), active: 0, activeDropped: true });
});

it("фоновая вкладка — после активной и уже открытых из неё, активная та же", () => {
  expect(openBehind({ tabs: list("/a", "/b"), active: 0 }, "/x", 0)).toEqual({ tabs: list("/a", "/x", "/b"), active: 0 });
  expect(openBehind({ tabs: list("/a", "/x", "/b"), active: 0 }, "/y", 1)).toEqual({ tabs: list("/a", "/x", "/y", "/b"), active: 0 });
  expect(openBehind({ tabs: list("/a"), active: 0 }, "/x", 5)).toEqual({ tabs: list("/a", "/x"), active: 0 });
});
