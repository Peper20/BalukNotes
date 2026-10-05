import { expect, it } from "vitest";
import type { BookView, SearchHit } from "./api";
import { inChapter, nextScope, scopeLabel, scopes } from "./find";

const hit = (anchor: string | null): SearchHit => ({ id: "Книга", kind: "book", title: "Книга", heading: anchor, anchor, snippet: [], score: 1 });

const book = (chapter: number): BookView => ({
  chapter,
  chapters: [
    { id: "г1", num: "1", title: "Первая" },
    { id: "г2", num: "2", title: "Вторая" },
  ],
  anchors: { г1: 0, "итоги": 0, г2: 1, "итоги-2": 1 },
});

it("a chapter scope only for a book by chapters", () => {
  expect(scopes(true)).toEqual(["chapter", "note", "vault"]);
  expect(scopes(false)).toEqual(["note", "vault"]);
  expect(scopeLabel("note", true)).toBe("Книга");
  expect(scopeLabel("note", false)).toBe("Заметка");
});

it("scopes in a circle; no chapter - from the note", () => {
  expect(nextScope("note", true, 1)).toBe("vault");
  expect(nextScope("vault", true, 1)).toBe("chapter");
  expect(nextScope("chapter", true, -1)).toBe("vault");
  expect(nextScope("vault", false, 1)).toBe("note");
  expect(nextScope("chapter", false, 1)).toBe("vault");
});

it("in a chapter, its sections; the start of the book is in the first chapter", () => {
  const hits = [hit(null), hit("итоги"), hit("г2"), hit("итоги-2"), hit("нет-такого")];
  expect(inChapter(hits, book(0)).map((h) => h.anchor)).toEqual([null, "итоги"]);
  expect(inChapter(hits, book(1)).map((h) => h.anchor)).toEqual(["г2", "итоги-2"]);
});
