import { expect, it } from "vitest";
import { clampActive, closeAt, cycle, dropTabs, openAfter, openBehind } from "./tabs";

const list = (...urls: string[]) => urls.map((url) => ({ url }));

it("a new tab goes right after the active one", () => {
  expect(openAfter({ tabs: list("/a", "/b", "/c"), active: 0 }, "/x")).toEqual({ tabs: list("/a", "/x", "/b", "/c"), active: 1 });
  expect(openAfter({ tabs: list("/a"), active: 0 }, "/x")).toEqual({ tabs: list("/a", "/x"), active: 1 });
});

it("closing a tab: the active one stays, a closed active one gives the neighbour", () => {
  const s = { tabs: list("/a", "/b", "/c"), active: 1 };
  expect(closeAt(s, 0)).toEqual({ tabs: list("/b", "/c"), active: 0, wasActive: false });
  expect(closeAt(s, 2)).toEqual({ tabs: list("/a", "/b"), active: 1, wasActive: false });
  expect(closeAt(s, 1)).toEqual({ tabs: list("/a", "/c"), active: 1, wasActive: true });
  expect(closeAt({ tabs: list("/a", "/b"), active: 1 }, 1)).toEqual({ tabs: list("/a"), active: 0, wasActive: true });
  expect(closeAt({ tabs: list("/a"), active: 0 }, 0)).toBeNull();
  expect(closeAt(s, 5)).toBeNull();
});

it("in a circle and within limits", () => {
  expect(cycle(2, 3, 1)).toBe(0);
  expect(cycle(0, 3, -1)).toBe(2);
  expect(clampActive(7, 3)).toBe(2);
  expect(clampActive(-1, 3)).toBe(0);
});

it("removing the tabs of a deleted note", () => {
  const isA = (t: { url: string }) => t.url.startsWith("/a");
  // The active one remained: it stays active.
  expect(dropTabs({ tabs: list("/a", "/b", "/a#x", "/c"), active: 3 }, isA, "/")).toEqual({ tabs: list("/b", "/c"), active: 1, activeDropped: false });
  // The active one was removed: the nearest on the left; none on the left - on the right.
  expect(dropTabs({ tabs: list("/b", "/a", "/c"), active: 1 }, isA, "/")).toEqual({ tabs: list("/b", "/c"), active: 0, activeDropped: true });
  expect(dropTabs({ tabs: list("/a", "/c"), active: 0 }, isA, "/")).toEqual({ tabs: list("/c"), active: 0, activeDropped: true });
  // None left: one new tab.
  expect(dropTabs({ tabs: list("/a", "/a#y"), active: 1 }, isA, "/home")).toEqual({ tabs: list("/home"), active: 0, activeDropped: true });
});

it("a background tab goes after the active one and those already opened from it, the active one stays", () => {
  expect(openBehind({ tabs: list("/a", "/b"), active: 0 }, "/x", 0)).toEqual({ tabs: list("/a", "/x", "/b"), active: 0 });
  expect(openBehind({ tabs: list("/a", "/x", "/b"), active: 0 }, "/y", 1)).toEqual({ tabs: list("/a", "/x", "/y", "/b"), active: 0 });
  expect(openBehind({ tabs: list("/a"), active: 0 }, "/x", 5)).toEqual({ tabs: list("/a", "/x"), active: 0 });
});
