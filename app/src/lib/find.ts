// The Ctrl+F search (the palette): where to search - the shown chapter of a
// book, the whole note (the whole book) or the whole vault. The browser's
// Ctrl+F sees only the shown chapter, so the note search comes from the
// server (a second Ctrl+F is the browser's search).

import type { BookView, SearchHit } from "./api";

export type FindScope = "chapter" | "note" | "vault";

/** Search from the note `note`: where to search now. */
export interface Find {
  note: string;
  scope: FindScope;
}

/** Where one can search: a chapter only in a book shown by chapters. */
export const scopes = (byChapters: boolean): FindScope[] => (byChapters ? ["chapter", "note", "vault"] : ["note", "vault"]);

export const scopeLabel = (scope: FindScope, isBook: boolean): string =>
  scope === "chapter" ? "Глава" : scope === "note" ? (isBook ? "Книга" : "Заметка") : "Всё хранилище";

/** The next (`delta` = 1) or previous scope in a circle. */
export function nextScope(scope: FindScope, byChapters: boolean, delta: number): FindScope {
  const all = scopes(byChapters);
  const i = Math.max(all.indexOf(scope), 0);
  return all[(i + delta + all.length) % all.length]!;
}

/** Sections of the shown chapter; the start of the book (a section without an anchor) is in the first chapter. */
export const inChapter = (hits: SearchHit[], book: BookView): SearchHit[] =>
  hits.filter((h) => (h.anchor == null ? 0 : book.anchors[h.anchor]) === book.chapter);
