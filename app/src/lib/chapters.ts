// Соседние главы книги — заранее: переход «следующая / предыдущая глава»
// без ожидания сервера (на медленной сети — заметная пауза). Держим только
// показанную главу и её соседей той же версии книги. Перед переходом —
// сверка версии книги (`fresh`): показанная глава могла устареть (событие
// ещё не дошло, режим «по кнопке»), а главы разных версий не смешиваются —
// номера, ссылки и счётчики у книги общие.

import type { BookView, Chapter, NotePage } from "./api";

export type FetchChapter = (id: string, chapter: number, signal: AbortSignal) => Promise<NotePage>;

/** Глава, в которой раздел `anchor` (результат поиска по книге), — `null`, если не знаем. */
export function chapterOf(book: BookView | null, anchor: string | null): Chapter | null {
  if (!book || anchor == null) return null;
  const k = book.anchors[anchor];
  return k == null ? null : (book.chapters[k] ?? null);
}

/** Подпись главы в результатах поиска: «гл. 2», у главы без номера — её название. */
export const chapterLabel = (c: Chapter): string => (c.num ? `гл. ${c.num}` : c.title);

/** Соседи главы `k` из `count` глав. */
export const neighbours = (k: number, count: number): number[] => [k - 1, k + 1].filter((c) => c >= 0 && c < count);

export class ChapterCache {
  #id: string | null = null;
  #version: string | null = null;
  #pages = new Map<number, NotePage>();
  #ctrl: AbortController | null = null;
  #timer: ReturnType<typeof setTimeout> | undefined;

  /** `delay` — пауза перед загрузкой соседей: сначала — всё для показанной главы. */
  constructor(
    private readonly fetch: FetchChapter,
    private readonly delay = 300,
  ) {}

  /** Глава из запаса — если она той же версии книги, что показанная. */
  get(id: string, chapter: number, version: string | null): NotePage | null {
    if (id !== this.#id || version == null || version !== this.#version) return null;
    return this.#pages.get(chapter) ?? null;
  }

  /**
   * Глава из запаса для перехода, если книга не изменилась: `current` —
   * версия книги сейчас (сверка с сервером), `null` — сверить не вышло
   * (берём запас). Изменилась — запас выброшен, глава — с сервера.
   */
  fresh(id: string, chapter: number, shown: string | null, current: string | null): NotePage | null {
    if (current != null && current !== shown) {
      this.clear();
      return null;
    }
    return this.get(id, chapter, shown);
  }

  /** Показана глава `page`: её и соседей — держать, остальных — забыть, недостающих соседей — загрузить. */
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
          // Пока грузили, книгу пересобрали или ушли с неё — не держать.
          if (ctrl.signal.aborted || got.version !== this.#version || got.book?.chapter !== k) return;
          this.#pages.set(k, got);
        } catch {
          return; // отменено или сервер недоступен — перейдём обычным запросом
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

  /** Какие главы в запасе (для тестов). */
  get held(): number[] {
    return [...this.#pages.keys()].sort((a, b) => a - b);
  }
}
