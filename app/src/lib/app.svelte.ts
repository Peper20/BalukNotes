// Состояние клиента: маршрут, список заметок, показанная заметка, настройки.
//
// Сервер отдаёт готовый HTML заметки (/api/notes/…); клиент вставляет его и
// применяет настройки вида атрибутами на <html>. Книгу (настройка
// `books.pages`) сервер отдаёт по главе: `?chapter=N` или `?anchor=`. Обновление — по кнопке и
// раз в N секунд сверкой версии (/api/version/… — дёшево: сервер ничего не
// компилирует, если файлы не менялись).
//
// <html data-state="loading|ready">: заметка или главная дорисована. По нему
// ждут tools/visual.mjs и e2e-тесты — договорённость для любого клиента.

import { api, ApiError, type ChapterSelect, type NoteListItem, type NotePage, type Schema, type SettingValues, type Theme } from "./api";
import { applyAppearance, resolveTheme } from "./appearance";
import { hashAnchor, noteHref, parseRoute, type Route } from "./ids";
import { load, save } from "./storage";

/** Место чтения: прокрутка и глава книги. */
export interface Place {
  y: number;
  chapter: number | null;
}

/** Как прокрутить только что показанную заметку. */
export type ScrollIntent = { mode: "top" } | { mode: "anchor" } | ({ mode: "keep" } & Place);

/** Вкладка — адрес (путь и якорь) внутри клиента. */
export interface Tab {
  url: string;
}

const MAX_RECENT = 30;
const MAX_PLACES = 200;

class App {
  notes = $state.raw<NoteListItem[]>([]);
  themes = $state.raw<Theme[]>([]);
  schema = $state.raw<Schema | null>(null);
  settings = $state.raw<SettingValues>({});
  systemDark = $state(false);
  settingsError = $state<string | null>(null);

  route = $state.raw<Route>({ kind: "home" });
  /** Якорь адреса; `anchorSeq` растёт при переходе по якорю в той же заметке. */
  anchor = $state<string | null>(null);
  anchorSeq = $state(0);

  /** Показанная заметка — или null, пока собирается / не загрузилась. */
  page = $state.raw<NotePage | null>(null);
  /** Почему заметку не показать (нет такой, сервер недоступен). */
  failure = $state<string | null>(null);
  /** Заметка, которая сейчас загружается, и с какого момента. */
  pending = $state<{ id: string; since: number } | null>(null);
  status = $state("");
  busy = $state(false);

  /** Вкладки и активная; адрес активной — всегда адрес страницы. */
  tabs = $state<Tab[]>(load<{ tabs: Tab[] }>("k-tabs", { tabs: [] }).tabs);
  activeTab = $state(load<{ active: number }>("k-tabs", { active: 0 }).active);
  /** Недавно открытые заметки, свежие первыми. */
  recent = $state<string[]>(load<string[]>("k-recent", []));

  /** Прокрутка для следующей показанной заметки (читает NoteView). */
  scroll: ScrollIntent = { mode: "top" };
  /** Место, куда вернуться в уже показанной заметке («назад» без якоря). */
  restore: Place | null = null;
  /** Текущая глава книги (ставит NoteView) — для «перезагрузить, не сбив место». */
  chapter: number | null = null;

  theme = $derived(resolveTheme(this.settings["appearance.theme"], this.themes, this.systemDark));
  currentId = $derived(this.route.kind === "note" ? this.route.id : null);
  currentNote = $derived(this.notes.find((n) => n.id === this.currentId));
  /** Пункты оглавления: `panels.toc_depth` уровней от верхнего; меньше двух — оглавления нет. */
  toc = $derived.by(() => {
    const headings = this.page?.rendered?.headings ?? [];
    const top = Math.min(...headings.map((h) => h.level));
    const depth = Number(this.settings["panels.toc_depth"] ?? 2);
    const shown = headings.filter((h) => h.level - top < depth).map((h) => ({ ...h, depth: h.level - top }));
    return shown.length >= 2 ? shown : [];
  });

  #ctrl: AbortController | null = null;
  #version: string | null = null;
  #timer: ReturnType<typeof setInterval> | undefined;
  /** Место для заметки, которая сейчас загружается (из истории или памяти). */
  #place: Place | null = null;
  /** Заметка → где её читали в последний раз. */
  #places = load<Record<string, Place>>("k-places", {});

  async init(): Promise<void> {
    const dark = matchMedia("(prefers-color-scheme: dark)");
    this.systemDark = dark.matches;
    dark.addEventListener("change", () => (this.systemDark = dark.matches));
    const [settings, themes, notes] = await Promise.all([api.settings(), api.themes(), api.notes()]);
    this.schema = settings.schema;
    this.settings = settings.values;
    this.themes = themes;
    this.notes = notes;
    this.#schedule();
    history.scrollRestoration = "manual";
    if (!this.tabs.length) this.tabs = [{ url: "/" }];
    this.activeTab = Math.min(Math.max(this.activeTab, 0), this.tabs.length - 1);
    this.syncRoute();
    addEventListener("popstate", () => this.syncRoute({ pop: true }));
    addEventListener("focus", () => this.settings["refresh.on_focus"] && this.check());
    addEventListener("pagehide", () => this.remember());
  }

  applyAppearance(root: HTMLElement): void {
    applyAppearance(root, this.settings, this.theme);
  }

  // ── Навигация ────────────────────────────────────────────────────────────

  /** Перейти внутри клиента (как по ссылке); `newTab` — в новой вкладке. */
  go(url: string | URL, { replace = false, newTab = false } = {}): void {
    this.remember();
    if (newTab) {
      this.tabs.splice(this.activeTab + 1, 0, { url: String(url) });
      this.activeTab++;
    }
    if (replace) history.replaceState(null, "", url);
    else history.pushState(null, "", url);
    this.syncRoute();
  }

  open(id: string, anchor?: string | null, { newTab = false } = {}): void {
    this.go(noteHref(id, anchor), { newTab });
  }

  switchTab(i: number): void {
    const tab = this.tabs[i];
    if (!tab || i === this.activeTab) return;
    this.remember();
    this.activeTab = i;
    history.pushState(null, "", tab.url);
    this.syncRoute();
  }

  closeTab(i: number): void {
    if (i < 0 || i >= this.tabs.length) return;
    if (this.tabs.length === 1) {
      this.go("/");
      return;
    }
    const wasActive = i === this.activeTab;
    this.tabs.splice(i, 1);
    if (i < this.activeTab || this.activeTab >= this.tabs.length) this.activeTab--;
    if (wasActive) {
      history.replaceState(null, "", this.tabs[this.activeTab]!.url);
      this.syncRoute();
    }
    this.#saveTabs();
  }

  /**
   * Запомнить место в текущей заметке: в записи истории (для «назад») и по
   * заметке (для нового открытия). Зовётся перед переходом и по ходу
   * прокрутки: к `popstate` запись истории уже сменилась.
   */
  remember(): void {
    if (!this.page || this.page.id !== this.currentId) return;
    const place: Place = { y: Math.round(scrollY), chapter: this.chapter };
    history.replaceState({ ...(history.state as object | null), ...place }, "");
    delete this.#places[this.page.id];
    this.#places[this.page.id] = place;
    const ids = Object.keys(this.#places);
    for (const id of ids.slice(0, Math.max(0, ids.length - MAX_PLACES))) delete this.#places[id];
    save("k-places", this.#places);
  }

  #saveTabs(): void {
    save("k-tabs", { tabs: this.tabs, active: this.activeTab });
  }

  /** Маршрут по адресу страницы: та же заметка — только якорь. */
  syncRoute({ pop = false } = {}): void {
    const route = parseRoute(location.pathname);
    this.anchor = hashAnchor(location.hash);
    const tab = this.tabs[this.activeTab];
    if (tab) tab.url = location.pathname + location.search + location.hash;
    this.#saveTabs();
    // «Назад» — туда, где были в этой записи истории; иначе — где читали
    // эту заметку в прошлый раз (если адрес без якоря).
    const state = history.state as Partial<Place> | null;
    const fromHistory = pop && typeof state?.y === "number" ? { y: state.y, chapter: state.chapter ?? null } : null;
    if (route.kind === "note" && route.id === this.currentId && !this.pending) {
      this.restore = this.anchor ? null : fromHistory;
      this.anchorSeq++;
      return;
    }
    this.route = route;
    if (route.kind === "note") {
      this.#place = fromHistory ?? (this.anchor ? null : (this.#places[route.id] ?? null));
      void this.load(route.id);
    } else this.#showHome();
  }

  #showHome(): void {
    this.#cancel();
    this.page = null;
    this.failure = null;
    this.#version = null;
    document.title = "Заметки";
  }

  // ── Заметка ──────────────────────────────────────────────────────────────

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
      this.#version = null;
      keepScroll = false;
      scrollTo(0, 0);
    }
    this.failure = null;
    this.pending = { id, since: Date.now() };
    document.documentElement.dataset.state = "loading";
    this.#setStatus("собираю…", true);
    document.title = `${this.currentNote?.name ?? id} — Заметки`;
    const place = this.#place;
    this.#place = null;
    try {
      const page = await api.note(id, ctrl.signal, this.#chapterSelect({ keepScroll, chapter, place }));
      if (this.#ctrl !== ctrl) return;
      this.#version = page.version;
      this.scroll =
        scroll ??
        (keepScroll
          ? { mode: "keep", y: scrollY, chapter: this.chapter }
          : this.anchor
            ? { mode: "anchor" }
            : place
              ? { mode: "keep", ...place }
              : { mode: "top" });
      this.page = page;
      this.recent = [id, ...this.recent.filter((r) => r !== id)].slice(0, MAX_RECENT);
      save("k-recent", this.recent);
      document.title = `${page.rendered?.title ?? this.currentNote?.name ?? id} — Заметки`;
      this.#setStatus(`собрано ${new Date().toLocaleTimeString("ru-RU")}`);
    } catch (e) {
      if (this.#ctrl !== ctrl) return; // отменена или устарела
      this.#version = null;
      this.page = null;
      this.failure =
        e instanceof ApiError && e.status === 404
          ? `Заметки «${id}» нет.`
          : `Не удалось загрузить: ${(e as Error).message}`;
      this.#setStatus("");
      document.documentElement.dataset.state = "ready";
    } finally {
      if (this.#ctrl === ctrl) {
        this.#ctrl = null;
        this.pending = null;
      }
    }
  }

  /** Какую главу просить у сервера (не книга — сервер отдаст целиком). */
  #chapterSelect({ keepScroll, chapter, place }: { keepScroll: boolean; chapter?: number; place: Place | null }): ChapterSelect {
    if (this.settings["books.pages"] !== "chapters") return {};
    if (chapter != null) return { chapter };
    if (keepScroll && this.chapter != null) return { chapter: this.chapter };
    if (this.anchor) return { anchor: this.anchor };
    return { chapter: place?.chapter ?? 0 };
  }

  /** Перейти к главе показанной книги. */
  showChapter(chapter: number, scroll: ScrollIntent): void {
    if (this.currentId) void this.load(this.currentId, { chapter, scroll });
  }

  #setStatus(text: string, busy = false): void {
    this.status = text;
    this.busy = busy;
  }

  // ── Обновление ───────────────────────────────────────────────────────────

  /**
   * Список заметок с сервера. Заметки создаёт Claude Code в любой момент —
   * дерево обновляется при каждой проверке, без перезагрузки страницы.
   */
  async refreshNotes(): Promise<void> {
    try {
      const notes = await api.notes();
      if (JSON.stringify(notes) !== JSON.stringify(this.notes)) this.notes = notes;
    } catch {
      // сервер недоступен — попробуем в следующий раз
    }
  }

  /** Изменились ли файлы заметки — и если да, перезагрузить её. */
  async check({ force = false } = {}): Promise<void> {
    void this.refreshNotes();
    const id = this.currentId;
    if (!id || this.pending) return;
    if (force) return this.load(id, { keepScroll: true });
    try {
      const { version } = await api.version(id);
      // Пока ждали ответ, могли перейти на другую заметку.
      if (this.currentId === id && !this.pending && version !== this.#version) await this.load(id, { keepScroll: true });
    } catch {
      // сервер недоступен — попробуем в следующий раз
    }
  }

  #schedule(): void {
    clearInterval(this.#timer);
    const seconds = Number(this.settings["refresh.interval"]);
    if (seconds > 0) this.#timer = setInterval(() => document.hidden || this.check(), seconds * 1000);
  }

  // ── Настройки ────────────────────────────────────────────────────────────

  async saveSettings(patch: SettingValues): Promise<void> {
    try {
      this.settings = await api.saveSettings(patch);
      this.settingsError = null;
      this.#schedule();
      // Настройки отрисовки (figures.*) меняют версию страницы на сервере,
      // вид книги (books.*) — раскладку уже полученной страницы.
      const keys = Object.keys(patch);
      if (keys.some((k) => k.startsWith("figures."))) void this.check();
      else if (this.currentId && keys.some((k) => k.startsWith("books."))) void this.load(this.currentId, { keepScroll: true });
    } catch (e) {
      this.settingsError = (e as Error).message;
    }
  }

  cycleTheme(): void {
    const options = ["auto", ...this.themes.map((t) => t.name)];
    const i = options.indexOf(String(this.settings["appearance.theme"]));
    void this.saveSettings({ "appearance.theme": options[(i + 1) % options.length]! });
  }

  pdfUrl(): string | null {
    return this.currentId ? api.pdfUrl(this.currentId, this.theme) : null;
  }
}

export const app = new App();
