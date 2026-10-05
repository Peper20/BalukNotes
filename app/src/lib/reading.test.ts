import { expect, it } from "vitest";
import { chapterSelect, scrollIntent, tocItems } from "./reading";

const none = { keepScroll: false, current: null, anchor: null, place: null };

it("chapter: only if the book is by chapters", () => {
  expect(chapterSelect(false, { ...none, chapter: 3 })).toEqual({});
  expect(chapterSelect(true, { ...none, chapter: 3 })).toEqual({ chapter: 3 });
  expect(chapterSelect(true, { ...none, keepScroll: true, current: 2 })).toEqual({ chapter: 2 });
  expect(chapterSelect(true, { ...none, anchor: "x" })).toEqual({ anchor: "x" });
  expect(chapterSelect(true, { ...none, place: { y: 0, chapter: 4 } })).toEqual({ chapter: 4 });
  expect(chapterSelect(true, none)).toEqual({ chapter: 0 });
});

it("scroll after loading", () => {
  const base = { ...none, y: 120 };
  expect(scrollIntent({ mode: "top" }, { ...base, keepScroll: true })).toEqual({ mode: "top" });
  expect(scrollIntent(undefined, { ...base, keepScroll: true, current: 1 })).toEqual({ mode: "keep", y: 120, chapter: 1 });
  expect(scrollIntent(undefined, { ...base, anchor: "a", place: { y: 5, chapter: null } })).toEqual({ mode: "anchor" });
  expect(scrollIntent(undefined, { ...base, place: { y: 5, chapter: null } })).toEqual({ mode: "keep", y: 5, chapter: null });
  expect(scrollIntent(undefined, base)).toEqual({ mode: "top" });
});

it("outline: levels from the top, fewer than two items - none", () => {
  const h = (level: number, id: string) => ({ level, id, anchor: id, text: id });
  const heads = [h(2, "a"), h(3, "b"), h(4, "c"), h(2, "d")];
  expect(tocItems(heads, 2).map((x) => [x.id, x.depth])).toEqual([["a", 0], ["b", 1], ["d", 0]]);
  expect(tocItems(heads, 1).map((x) => x.id)).toEqual(["a", "d"]);
  expect(tocItems([h(2, "a")], 3)).toEqual([]);
  expect(tocItems([], 3)).toEqual([]);
});
