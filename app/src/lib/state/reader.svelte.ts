// The shown note: loading from the server (a book by chapter), build status,
// scroll and the reading place. NoteView inserts the HTML.
//
// <html data-state="loading|ready">: a note or the home page is drawn.
// tools/visual.mjs and the e2e tests wait for it - a contract for any client.

import { api, ApiError, type NotePage } from "../api";
import { scrollToTop, scrollTop } from "../scroll";
import { ChapterCache } from "../chapters";
import type { Place } from "../places";
import { chapterSelect, scrollIntent, tocItems, type ScrollIntent } from "../reading";
import { notes } from "./notes.svelte";
import { places } from "./places.svelte";
import { router } from "./router.svelte";
import { settings } from "./settings.svelte";

/** How long the previous note stays on screen while the new one loads. */
const STALE_MS = 150;

class Reader {
  /** The shown note, or null while it builds / failed to load. */
  page = $state.raw<NotePage | null>(null);
  /** Why the note cannot be shown (no such note, the server is unreachable). */
  failure = $state<string | null>(null);
  /** The note being loaded now, and since when. */
  pending = $state<{ id: string; since: number } | null>(null);
  status = $state("");
  busy = $state(false);

  /** Scroll for the next shown note (NoteView reads it). */
  scroll: ScrollIntent = { mode: "top" };
  /** Where to return in the note already shown ("back" without an anchor). */
  restore: Place | null = null;
  /** The current book chapter (set by NoteView), for "reload without losing the place". */
  chapter: number | null = null;
  /** Version of the shown note (the update check compares it). */
  version: string | null = null;

  /** Outline items: `panels.toc_depth` levels from the top. */
  toc = $derived(tocItems(this.page?.rendered?.headings ?? [], Number(settings.values["panels.toc_depth"] ?? 2)));

  #ctrl: AbortController | null = null;
  /** Neighbouring chapters of the shown book, in advance. */
  #chapters = new ChapterCache((id, chapter, signal) => api.note(id, signal, { chapter }));
  /** Place for the note being loaded now (from the history or memory). */
  #place: Place | null = null;

  /** Shows the note from the place `place` (null - by the anchor or from the start). */
  show(id: string, place: Place | null): void {
    this.#place = place;
    void this.load(id);
  }

  /** No note on screen (home, graph, tags). */
  clear(): void {
    this.#cancel();
    this.page = null;
    this.failure = null;
    this.version = null;
    this.#chapters.clear();
    document.title = "Заметки";
  }

  #cancel(): void {
    this.#ctrl?.abort();
    this.#ctrl = null;
    this.pending = null;
  }

  /**
   * Shows a note. A big note builds for seconds; if another one is chosen
   * meanwhile, the previous request is cancelled and its response (if it
   * came) is dropped - otherwise the client would "jump" back.
   *
   * `chapter` and `scroll`: go to a book chapter (it is already built, the
   * response comes from the server cache) and how to scroll it.
   */
  async load(id: string, { keepScroll = false, chapter, scroll }: { keepScroll?: boolean; chapter?: number; scroll?: ScrollIntent } = {}): Promise<void> {
    this.#cancel();
    const ctrl = new AbortController();
    this.#ctrl = ctrl;
    if (this.page?.id !== id) {
      // Another note. From the cache it comes in tens of ms, the previous one
      // stays until then, without an empty frame. If it builds longer, the
      // previous one goes: no foreign text under a new title on screen.
      const old = this.page;
      this.version = null;
      keepScroll = false;
      setTimeout(() => {
        if (this.#ctrl !== ctrl || this.page !== old) return;
        this.page = null;
        scrollToTop(0);
      }, STALE_MS);
    }
    this.failure = null;
    this.pending = { id, since: Date.now() };
    document.documentElement.dataset.state = "loading";
    this.setStatus("собираю…", true);
    document.title = `${notes.title(id)} — Заметки`;
    const place = this.#place;
    this.#place = null;
    const where = { keepScroll, current: this.chapter, anchor: router.anchor, place };
    const select = chapterSelect(settings.values["books.pages"] === "chapters", { ...where, chapter });
    // Another chapter of the shown book: first a cheap check of the book
    // version - unchanged, the chapter comes from the stock; changed, a new
    // version from the server.
    const k = select.chapter ?? (select.anchor != null ? this.page?.book?.anchors[select.anchor] : undefined);
    const shown = this.version;
    const toChapter = !keepScroll && k != null && this.page?.id === id;
    try {
      const current = toChapter ? await api.version(id).then((v) => v.version, () => null) : null;
      if (this.#ctrl !== ctrl) return;
      const ready = toChapter ? this.#chapters.fresh(id, k, shown, current) : null;
      const page = ready ?? (await api.note(id, ctrl.signal, select));
      if (this.#ctrl !== ctrl) return;
      const updated = toChapter && shown != null && page.version !== shown;
      this.version = page.version;
      this.scroll = scrollIntent(scroll, { ...where, y: scrollTop() });
      this.page = page;
      this.#chapters.shown(page);
      places.visited(id);
      document.title = `${page.rendered?.title ?? notes.title(id)} — Заметки`;
      this.setStatus(`${updated ? "книга обновлена" : "собрано"} ${new Date().toLocaleTimeString("ru-RU")}`);
    } catch (e) {
      if (this.#ctrl !== ctrl) return; // cancelled or stale
      const offline = e instanceof ApiError && e.offline;
      document.documentElement.dataset.state = "ready";
      // No connection while a note is on screen (an update, another chapter):
      // it stays, better the previous one than an empty page.
      if (offline && this.page?.id === id) {
        this.setStatus(""); // the label in Topbar shows "нет связи"
        return;
      }
      this.version = null;
      this.page = null;
      this.failure = offline
        ? "Нет связи с сервером: заметка откроется, когда связь вернётся."
        : e instanceof ApiError && e.status === 404
          ? `Заметки «${id}» нет.`
          : `Не удалось загрузить: ${(e as Error).message}`;
      this.setStatus("");
    } finally {
      if (this.#ctrl === ctrl) {
        this.#ctrl = null;
        this.pending = null;
      }
    }
  }

  /** Goes to a chapter of the shown book. */
  showChapter(chapter: number, scroll: ScrollIntent): void {
    if (router.currentId) void this.load(router.currentId, { chapter, scroll });
  }

  /** The same note anew without losing the place (a file edit, "refresh"). */
  reload(): Promise<void> {
    return router.currentId ? this.load(router.currentId, { keepScroll: true }) : Promise.resolve();
  }

  /**
   * Remembers the place in the current note: in the history entry (for
   * "back") and per note (for a new opening). Called before a navigation and
   * while scrolling: by `popstate` the history entry has already changed.
   */
  remember(): void {
    if (!this.page || this.page.id !== router.currentId) return;
    const place: Place = { y: Math.round(scrollTop()), chapter: this.chapter };
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
