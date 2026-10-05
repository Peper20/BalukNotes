// App commands: one place for the palette (Ctrl+K), shortcuts and the help
// (?). A new interface feature = a command here.

import { api } from "./api";
import { notes, places, reader, router, settings, tabs, updates } from "./state";
import { graphHref, homeHref, noteHref, parseRoute, tagHref } from "./ids";
import { combo, type Combo } from "./keys";
import { movedId } from "./rename";
import { mobile, ui } from "./ui.svelte";

export interface Command {
  id: string;
  title: string;
  group: string;
  /** Shortcuts; the first one is shown in the palette and the help. */
  keys?: Combo[];
  /** Whether it is available now (e.g. only with an open note). */
  available?: () => boolean;
  /** Whether the keys work now (otherwise they go to the browser); `available` by default. */
  keysAvailable?: () => boolean;
  run: () => void;
}

const keys = (...specs: string[]) => specs.map(combo);
const hasNote = () => reader.page != null;
const hasBook = () => ui.book != null;

/** Go to a book chapter via the anchor of its heading (gets into the history). */
function chapter(delta: number) {
  const book = ui.book;
  const target = book?.chapters[ui.chapter + delta];
  if (target) router.go(`#${encodeURIComponent(target.id)}`);
}

function fontSize(delta: number) {
  const size = Number(settings.values["appearance.font_size"] ?? 19) + delta;
  void settings.save({ "appearance.font_size": Math.min(Math.max(size, 12), 32) });
}

async function copyLink() {
  const id = router.currentId;
  if (!id) return;
  await navigator.clipboard?.writeText(`#see("${id}")`).catch(() => {});
  reader.status = `скопировано: #see("${id}")`;
}

function toggleSidebar() {
  if (mobile.matches) ui.sidebarOpen = !ui.sidebarOpen;
  else ui.sidebarHidden = !ui.sidebarHidden;
}

/** §: on a wide screen hides/shows the side outline, on a narrow one the popup one. */
export function toggleToc() {
  if (!reader.toc.length) return;
  if (ui.tocRoom && !ui.tocOpen) void settings.save({ "panels.toc": !settings.values["panels.toc"] });
  else ui.tocOpen = !ui.tocOpen;
}

/** All commands; the themes follow the server's theme list. */
export function commands(): Command[] {
  const list: Command[] = [
    { id: "open", group: "Переход", title: "Быстрый переход к заметке", keys: keys("Ctrl+KeyO", "Ctrl+KeyP"), run: () => ui.openPalette("") },
    { id: "search", group: "Переход", title: "Поиск по тексту всех заметок", keys: keys("Ctrl+Shift+KeyF"), run: () => ui.openPalette("/") },
    {
      id: "find",
      group: "Переход",
      title: "Поиск в этой заметке или книге",
      // A book is shown by chapters: the browser's Ctrl+F sees one chapter, so
      // instead search the note, the chapter or the vault (a second Ctrl+F is
      // the browser's search, Palette).
      keys: keys("Ctrl+KeyF"),
      available: () => router.currentId != null,
      run: () => ui.openPalette("", { note: router.currentId!, scope: "note" }),
    },
    { id: "commands", group: "Переход", title: "Команды", keys: keys("Ctrl+KeyK"), run: () => ui.openPalette(">") },
    { id: "home", group: "Переход", title: "Главная: граф и все заметки", keys: keys("KeyH"), run: () => router.go(homeHref()) },
    { id: "tags", group: "Переход", title: "Теги", run: () => router.go(tagHref()) },
    { id: "graph", group: "Переход", title: "Граф заметок", keys: keys("KeyG"), run: () => router.go(graphHref()) },
    { id: "graph-around", group: "Переход", title: "Граф: соседи заметки", available: () => router.currentId != null, run: () => router.go(graphHref(router.currentId)) },
    { id: "prev-chapter", group: "Переход", title: "Предыдущая глава", keys: keys("BracketLeft"), available: hasBook, run: () => chapter(-1) },
    { id: "next-chapter", group: "Переход", title: "Следующая глава", keys: keys("BracketRight"), available: hasBook, run: () => chapter(1) },

    { id: "new-tab", group: "Вкладки", title: "Новая вкладка", keys: keys("Alt+KeyT"), run: () => router.go(homeHref(), { newTab: true }) },
    { id: "close-tab", group: "Вкладки", title: "Закрыть вкладку", keys: keys("Alt+KeyW"), run: () => router.closeTab(tabs.active) },
    { id: "next-tab", group: "Вкладки", title: "Следующая вкладка", keys: keys("Alt+BracketRight"), run: () => router.switchTab((tabs.active + 1) % tabs.list.length) },
    { id: "prev-tab", group: "Вкладки", title: "Предыдущая вкладка", keys: keys("Alt+BracketLeft"), run: () => router.switchTab((tabs.active - 1 + tabs.list.length) % tabs.list.length) },

    { id: "toc", group: "Вид", title: "Оглавление", keys: keys("KeyT"), available: () => reader.toc.length > 0, run: toggleToc },
    { id: "reading", group: "Вид", title: "Режим чтения: только текст", keys: keys("KeyF"), run: () => (ui.reading = !ui.reading) },
    { id: "sidebar", group: "Вид", title: "Панель заметок", keys: keys("Ctrl+Backslash"), run: toggleSidebar },
    { id: "bigger", group: "Вид", title: "Кегль крупнее", keys: keys("Equal"), run: () => fontSize(1) },
    { id: "smaller", group: "Вид", title: "Кегль мельче", keys: keys("Minus"), run: () => fontSize(-1) },
    { id: "theme", group: "Вид", title: "Тема: следующая", run: () => settings.cycleTheme() },
    { id: "theme-auto", group: "Вид", title: "Тема: как в системе", run: () => void settings.save({ "appearance.theme": "auto" }) },
    ...settings.themes.map((t) => ({
      id: `theme-${t.name}`,
      group: "Вид",
      title: `Тема: ${t.title}`,
      run: () => void settings.save({ "appearance.theme": t.name }),
    })),

    { id: "refresh", group: "Заметка", title: "Пересобрать заметку", keys: keys("KeyR"), available: () => router.currentId != null, run: () => void updates.check({ force: true }) },
    { id: "pdf", group: "Заметка", title: "PDF в текущей теме", available: hasNote, run: () => { const url = reader.pdfUrl(); if (url) open(url, "_blank"); } },
    { id: "copy-link", group: "Заметка", title: "Скопировать ссылку #see(…) на заметку", available: () => router.currentId != null, run: () => void copyLink() },
    { id: "rename-note", group: "Заметка", title: "Переименовать заметку…", available: () => router.currentNote != null, run: () => (ui.renaming = router.currentId ? { kind: "note", id: router.currentId } : null) },
    { id: "delete-note", group: "Заметка", title: "Удалить заметку…", available: () => router.currentNote != null, run: () => (ui.deleting = router.currentId ? { kind: "note", id: router.currentId } : null) },
    { id: "backlinks", group: "Заметка", title: "Кто ссылается сюда", available: hasNote, run: () => document.getElementById("backlinks")?.scrollIntoView({ behavior: "smooth" }) },

    { id: "settings", group: "Приложение", title: "Настройки", keys: keys("Ctrl+Comma"), run: () => (ui.settingsOpen = true) },
    { id: "help", group: "Приложение", title: "Горячие клавиши", keys: keys("?"), run: () => (ui.helpOpen = true) },
  ];
  for (let i = 1; i <= 9; i++) {
    list.push({ id: `tab-${i}`, group: "Вкладки", title: `Вкладка ${i}`, keys: keys(`Alt+Digit${i}`), available: () => tabs.list.length >= i, run: () => router.switchTab(i - 1) });
  }
  return list;
}

/**
 * Deletes a note (a book as a folder) to the system trash: closes its tabs,
 * forgets the reading place, refreshes the list; if it was open, shows the
 * neighbouring tab (or home).
 */
export async function deleteNote(id: string): Promise<void> {
  const title = notes.title(id);
  await api.deleteNote(id);
  const isIt = (url: string) => {
    const route = parseRoute(new URL(url, location.href).pathname);
    return route.kind === "note" && route.id === id;
  };
  if (tabs.drop((t) => isIt(t.url))) {
    history.replaceState(null, "", tabs.list[tabs.active]!.url);
    router.sync();
  }
  places.forget(id);
  await notes.refresh();
  reader.status = `в корзине: ${title}`;
}

/**
 * Deletes a folder with everything in it to the system trash: like
 * `deleteNote`, but for all notes of the folder.
 */
export async function deleteFolder(path: string): Promise<void> {
  const title = notes.folderTitle(path);
  const inside = (id: string) => id.startsWith(`${path}/`);
  const gone = notes.all.filter((n) => inside(n.id)).map((n) => n.id);
  await api.deleteFolder(path);
  const isIt = (url: string) => {
    const route = parseRoute(new URL(url, location.href).pathname);
    return route.kind === "note" && inside(route.id);
  };
  if (tabs.drop((t) => isIt(t.url))) {
    history.replaceState(null, "", tabs.list[tabs.active]!.url);
    router.sync();
  }
  for (const id of gone) places.forget(id);
  await notes.refresh();
  reader.status = `в корзине: папка ${title}`;
}

/**
 * Renames a note or a folder (the title, the file name and the links - the
 * server): tabs, reading places and collapsed folders move to the new paths;
 * an open one goes to its new address.
 */
export async function renameItem(kind: "note" | "folder", id: string, title: string): Promise<void> {
  const plan = await api.rename({ kind, id, title, apply: true });
  const moved = (path: string) => movedId(path, plan.from, plan.to);
  if (plan.from !== plan.to) {
    tabs.move((url) => {
      const u = new URL(url, location.href);
      const route = parseRoute(u.pathname);
      const to = route.kind === "note" ? moved(route.id) : null;
      return to == null ? null : noteHref(to) + u.search + u.hash;
    });
    places.move(moved);
    if (kind === "folder") ui.moveCollapsed(moved);
    history.replaceState(history.state, "", tabs.list[tabs.active]!.url);
  }
  await notes.refresh();
  router.sync();
  reader.status = `переименовано: ${plan.title}`;
}

/** Opens a note from the palette or a list: with Ctrl, in a new tab. */
export const openNote = (id: string, anchor: string | null, newTab: boolean) => router.go(noteHref(id, anchor), { newTab });
