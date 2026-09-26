import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { NotePage } from "./api";
import { ChapterCache, neighbours } from "./chapters";

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

it("соседи главы — в пределах книги", () => {
  expect(neighbours(0, 4)).toEqual([1]);
  expect(neighbours(2, 4)).toEqual([1, 3]);
  expect(neighbours(3, 4)).toEqual([2]);
  expect(neighbours(0, 1)).toEqual([]);
});

it("показанная глава — и её соседи заранее, той же версии", async () => {
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

  // Дальше вперёд: 0 забыта, 1 и 2 уже есть, догружается только 3.
  cache.shown(page(2));
  expect(cache.held).toEqual([1, 2]);
  await vi.advanceTimersByTimeAsync(100);
  expect(fetch.mock.calls.map((c) => c[1])).toEqual([0, 2, 3]);
  expect(cache.held).toEqual([1, 2, 3]);
});

it("новая версия книги — запас заново; устаревший ответ не держим", async () => {
  let version = "v1";
  const cache = new ChapterCache(async (_id, k) => page(k, version), 0);
  cache.shown(page(0));
  version = "v2"; // книгу пересобрали, пока грузили соседа
  await vi.advanceTimersByTimeAsync(0);
  expect(cache.held).toEqual([0]);
  cache.shown(page(0, "v2"));
  await vi.advanceTimersByTimeAsync(0);
  expect(cache.held).toEqual([0, 1]);
  cache.clear();
  expect(cache.held).toEqual([]);
});
