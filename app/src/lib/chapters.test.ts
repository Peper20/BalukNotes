import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { NotePage } from "./api";
import { ChapterCache, chapterLabel, chapterOf, neighbours } from "./chapters";

const page = (chapter: number, version = "v1", id = "Книга"): NotePage => ({
  id,
  kind: "book",
  version,
  rendered: null,
  errors: [],
  warnings: [],
  book: { chapter, chapters: Array.from({ length: 4 }, (_, i) => ({ id: `г${i}`, num: `${i + 1}`, title: `Глава ${i + 1}` })), anchors: {} },
});

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

it("the neighbours of a chapter stay within the book", () => {
  expect(neighbours(0, 4)).toEqual([1]);
  expect(neighbours(2, 4)).toEqual([1, 3]);
  expect(neighbours(3, 4)).toEqual([2]);
  expect(neighbours(0, 1)).toEqual([]);
});

it("the shown chapter and its neighbours in advance, of the same version", async () => {
  const fetch = vi.fn(async (_id: string, k: number) => page(k));
  const cache = new ChapterCache(fetch, 100);
  cache.shown(page(1));
  expect(fetch).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(100);
  expect(fetch.mock.calls.map((c) => c[1])).toEqual([0, 2]);
  expect(cache.held).toEqual([0, 1, 2]);
  expect(cache.get("Книга", 2, "v1")?.book?.chapter).toBe(2);
  expect(cache.get("Книга", 2, "v2")).toBeNull();
  expect(cache.get("Другая", 2, "v1")).toBeNull();

  // Further forward: 0 is forgotten, 1 and 2 are there, only 3 is loaded.
  cache.shown(page(2));
  expect(cache.held).toEqual([1, 2]);
  await vi.advanceTimersByTimeAsync(100);
  expect(fetch.mock.calls.map((c) => c[1])).toEqual([0, 2, 3]);
  expect(cache.held).toEqual([1, 2, 3]);
});

it("a new book version: the stock anew; a stale response is not kept", async () => {
  let version = "v1";
  const cache = new ChapterCache(async (_id, k) => page(k, version), 0);
  cache.shown(page(0));
  version = "v2"; // the book was rebuilt while the neighbour was loading
  await vi.advanceTimersByTimeAsync(0);
  expect(cache.held).toEqual([0]);
  cache.shown(page(0, "v2"));
  await vi.advanceTimersByTimeAsync(0);
  expect(cache.held).toEqual([0, 1]);
  cache.clear();
  expect(cache.held).toEqual([]);
});

it("going to a chapter: the book changed - the stock is dropped; the check failed - the stock", async () => {
  const cache = new ChapterCache(async (_id, k) => page(k), 0);
  cache.shown(page(0));
  await vi.advanceTimersByTimeAsync(0);
  expect(cache.fresh("Книга", 1, "v1", "v1")?.book?.chapter).toBe(1);
  expect(cache.fresh("Книга", 1, "v1", null)?.book?.chapter).toBe(1);
  expect(cache.fresh("Книга", 1, "v1", "v2")).toBeNull();
  expect(cache.held).toEqual([]);
});

it("the chapter of a section for book search results", () => {
  const book = { chapter: 0, chapters: [{ id: "г0", num: "1", title: "Основы" }, { id: "г1", num: "", title: "Приложение" }], anchors: { Итоги: 0, "Итоги-2": 1 } };
  expect(chapterOf(book, "Итоги")).toEqual(book.chapters[0]);
  expect(chapterOf(book, "Итоги-2")).toEqual(book.chapters[1]);
  expect(chapterOf(book, "нет")).toBeNull();
  expect(chapterOf(book, null)).toBeNull();
  expect(chapterOf(null, "Итоги")).toBeNull();
  expect(chapterLabel(book.chapters[0]!)).toBe("гл. 1");
  expect(chapterLabel(book.chapters[1]!)).toBe("Приложение");
});
