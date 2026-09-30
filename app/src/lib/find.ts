// Поиск Ctrl+F (палитра): где искать — показанная глава книги, вся заметка
// (книга целиком) или всё хранилище. Браузерный Ctrl+F видит только
// показанную главу, поэтому поиск по заметке — с сервера (повторное Ctrl+F —
// поиск браузера).

import type { BookView, SearchHit } from "./api";

export type FindScope = "chapter" | "note" | "vault";

/** Поиск из заметки `note`: где искать сейчас. */
export interface Find {
  note: string;
  scope: FindScope;
}

/** Где можно искать: глава — только у книги, показанной по главам. */
export const scopes = (byChapters: boolean): FindScope[] => (byChapters ? ["chapter", "note", "vault"] : ["note", "vault"]);

export const scopeLabel = (scope: FindScope, isBook: boolean): string =>
  scope === "chapter" ? "Глава" : scope === "note" ? (isBook ? "Книга" : "Заметка") : "Всё хранилище";

/** Следующая (`delta` = 1) или предыдущая область по кругу. */
export function nextScope(scope: FindScope, byChapters: boolean, delta: number): FindScope {
  const all = scopes(byChapters);
  const i = Math.max(all.indexOf(scope), 0);
  return all[(i + delta + all.length) % all.length]!;
}

/** Разделы показанной главы; начало книги (раздел без якоря) — в первой главе. */
export const inChapter = (hits: SearchHit[], book: BookView): SearchHit[] =>
  hits.filter((h) => (h.anchor == null ? 0 : book.anchors[h.anchor]) === book.chapter);
