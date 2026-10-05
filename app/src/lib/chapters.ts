// Neighbouring chapters of a book in advance: "next / previous chapter"
// without waiting for the server (a noticeable pause on a slow network). We
// keep only the shown chapter and its neighbours of the same book version.
// Before a navigation the book version is checked (`fresh`): the shown
// chapter may be stale (the event has not arrived yet, the "by button"
// mode), and chapters of different versions do not mix - numbers, links
// and counters are shared by the book.

import type { BookView, Chapter, NotePage } from "./api";

export type FetchChapter = (id: string, chapter: number, signal: AbortSignal) => Promise<NotePage>;

/** The chapter that has the section `anchor` (a book search result), `null` if unknown. */
export function chapterOf(book: BookView | null, anchor: string | null): Chapter | null {
  if (!book || anchor == null) return null;
  const k = book.anchors[anchor];
  return k == null ? null : (book.chapters[k] ?? null);
}

/** Chapter label in search results: "гл. 2", for a chapter without a number its title. */
export const chapterLabel = (c: Chapter): string => (c.num ? `гл. ${c.num}` : c.title);

/** Neighbours of chapter `k` of `count` chapters. */
export const neighbours = (k: number, count: number): number[] => [k - 1, k + 1].filter((c) => c >= 0 && c < count);

export class ChapterCache {
  #id: string | null = null;
  #version: string | null = null;
  #pages = new Map<number, NotePage>();
  #ctrl: AbortController | null = null;
  #timer: ReturnType<typeof setTimeout> | undefined;

  /** `delay` is the pause before loading the neighbours: first everything for the shown chapter. */
  constructor(
    private readonly fetch: FetchChapter,
    private readonly delay = 300,
  ) {}

  /** A chapter from the stock if it is of the same book version as the shown one. */
  get(id: string, chapter: number, version: string | null): NotePage | null {
    if (id !== this.#id || version == null || version !== this.#version) return null;
    return this.#pages.get(chapter) ?? null;
  }

  /**
   * A chapter from the stock for a navigation if the book did not change:
   * `current` is the book version now (checked with the server), `null` -
   * the check failed (we take the stock). Changed - the stock is dropped,
   * the chapter comes from the server.
   */
  fresh(id: string, chapter: number, shown: string | null, current: string | null): NotePage | null {
    if (current != null && current !== shown) {
      this.clear();
      return null;
    }
    return this.get(id, chapter, shown);
  }

  /** Chapter `page` is shown: keep it and its neighbours, forget the rest, load the missing neighbours. */
  shown(page: NotePage): void {
    const book = page.book;
    if (!book) return this.clear();
    if (page.id !== this.#id || page.version !== this.#version) {
      this.clear();
      this.#id = page.id;
      this.#version = page.version;
    }
    const keep = [book.chapter, ...neighbours(book.chapter, book.chapters.length)];
    for (const k of this.#pages.keys()) if (!keep.includes(k)) this.#pages.delete(k);
    this.#pages.set(book.chapter, page);
    const missing = keep.filter((k) => !this.#pages.has(k));
    clearTimeout(this.#timer);
    if (!missing.length) return;
    this.#ctrl ??= new AbortController();
    const [ctrl, id, version] = [this.#ctrl, page.id, page.version];
    this.#timer = setTimeout(async () => {
      for (const k of missing) {
        try {
          const got = await this.fetch(id, k, ctrl.signal);
          // The book was rebuilt or left while loading: do not keep it.
          if (ctrl.signal.aborted || got.version !== this.#version || got.book?.chapter !== k) return;
          this.#pages.set(k, got);
        } catch {
          return; // cancelled or the server is unreachable: we will go with a normal request
        }
      }
    }, this.delay);
  }

  clear(): void {
    clearTimeout(this.#timer);
    this.#ctrl?.abort();
    this.#ctrl = null;
    this.#pages.clear();
    this.#id = this.#version = null;
  }

  /** Which chapters are in stock (for tests). */
  get held(): number[] {
    return [...this.#pages.keys()].sort((a, b) => a - b);
  }
}
