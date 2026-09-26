// Показанная заметка: загрузка с сервера (книга — по главе), статус сборки,
// прокрутка и место чтения. HTML вставляет NoteView.
//
// <html data-state="loading|ready">: заметка или главная дорисована. По нему
// ждут tools/visual.mjs и e2e-тесты — договорённость для любого клиента.

import { api, ApiError, type NotePage } from "../api";
import type { Place } from "../places";
import { chapterSelect, scrollIntent, tocItems, type ScrollIntent } from "../reading";
import { places } from "./places.svelte";
import { router } from "./router.svelte";
import { settings } from "./settings.svelte";

class Reader {
  /** Показанная заметка — или null, пока собирается / не загрузилась. */
  page = $state.raw<NotePage | null>(null);
  /** Почему заметку не показать (нет такой, сервер недоступен). */
  failure = $state<string | null>(null);
  /** Заметка, которая сейчас загружается, и с какого момента. */
  pending = $state<{ id: string; since: number } | null>(null);
  status = $state("");
  busy = $state(false);

  /** Прокрутка для следующей показанной заметки (читает NoteView). */
  scroll: ScrollIntent = { mode: "top" };
  /** Место, куда вернуться в уже показанной заметке («назад» без якоря). */
  restore: Place | null = null;
  /** Текущая глава книги (ставит NoteView) — для «перезагрузить, не сбив место». */
  chapter: number | null = null;
  /** Версия показанной заметки (сверяет проверка обновлений). */
  version: string | null = null;

  /** Пункты оглавления: `panels.toc_depth` уровней от верхнего. */
  toc = $derived(tocItems(this.page?.rendered?.headings ?? [], Number(settings.values["panels.toc_depth"] ?? 2)));

  #ctrl: AbortController | null = null;
  /** Место для заметки, которая сейчас загружается (из истории или памяти). */
  #place: Place | null = null;

  /** Показать заметку с места `place` (null — по якорю или с начала). */
  show(id: string, place: Place | null): void {
    this.#place = place;
    void this.load(id);
  }

  /** Заметки нет на экране (главная, граф, теги). */
  clear(): void {
    this.#cancel();
    this.page = null;
    this.failure = null;
    this.version = null;
    document.title = "Заметки";
  }

  #cancel(): void {
    this.#ctrl?.abort();
    this.#ctrl = null;
    this.pending = null;
  }

  /**
   * Показать заметку. Большая заметка собирается секунды; если за это время
   * выбрали другую, прежний запрос отменяется, а его ответ (если успел)
   * отбрасывается — иначе клиент «перепрыгнул» бы назад.
   *
   * `chapter` и `scroll` — перейти к главе книги (она уже собрана: ответ —
   * из кэша сервера) и как её прокрутить.
   */
  async load(id: string, { keepScroll = false, chapter, scroll }: { keepScroll?: boolean; chapter?: number; scroll?: ScrollIntent } = {}): Promise<void> {
    this.#cancel();
    const ctrl = new AbortController();
    this.#ctrl = ctrl;
    if (this.page?.id !== id) {
      // Другая заметка: сразу убираем прежнюю — пока новая собирается, на
      // экране не должно быть чужого текста под новым заголовком.
      this.page = null;
      this.version = null;
      keepScroll = false;
      scrollTo(0, 0);
    }
    this.failure = null;
    this.pending = { id, since: Date.now() };
    document.documentElement.dataset.state = "loading";
    this.setStatus("собираю…", true);
    document.title = `${router.currentNote?.name ?? id} — Заметки`;
    const place = this.#place;
    this.#place = null;
    const where = { keepScroll, current: this.chapter, anchor: router.anchor, place };
    try {
      const page = await api.note(id, ctrl.signal, chapterSelect(settings.values["books.pages"] === "chapters", { ...where, chapter }));
      if (this.#ctrl !== ctrl) return;
      this.version = page.version;
      this.scroll = scrollIntent(scroll, { ...where, y: scrollY });
      this.page = page;
      places.visited(id);
      document.title = `${page.rendered?.title ?? router.currentNote?.name ?? id} — Заметки`;
      this.setStatus(`собрано ${new Date().toLocaleTimeString("ru-RU")}`);
    } catch (e) {
      if (this.#ctrl !== ctrl) return; // отменена или устарела
      this.version = null;
      this.page = null;
      this.failure = e instanceof ApiError && e.status === 404 ? `Заметки «${id}» нет.` : `Не удалось загрузить: ${(e as Error).message}`;
      this.setStatus("");
      document.documentElement.dataset.state = "ready";
    } finally {
      if (this.#ctrl === ctrl) {
        this.#ctrl = null;
        this.pending = null;
      }
    }
  }

  /** Перейти к главе показанной книги. */
  showChapter(chapter: number, scroll: ScrollIntent): void {
    if (router.currentId) void this.load(router.currentId, { chapter, scroll });
  }

  /** Та же заметка заново, не сбив место (правка файла, «обновить»). */
  reload(): Promise<void> {
    return router.currentId ? this.load(router.currentId, { keepScroll: true }) : Promise.resolve();
  }

  /**
   * Запомнить место в текущей заметке: в записи истории (для «назад») и по
   * заметке (для нового открытия). Зовётся перед переходом и по ходу
   * прокрутки: к `popstate` запись истории уже сменилась.
   */
  remember(): void {
    if (!this.page || this.page.id !== router.currentId) return;
    const place: Place = { y: Math.round(scrollY), chapter: this.chapter };
    history.replaceState({ ...(history.state as object | null), ...place }, "");
    places.remember(this.page.id, place);
  }

  setStatus(text: string, busy = false): void {
    this.status = text;
    this.busy = busy;
  }

  pdfUrl(): string | null {
    return router.currentId ? api.pdfUrl(router.currentId, settings.theme) : null;
  }
}

export const reader = new Reader();
