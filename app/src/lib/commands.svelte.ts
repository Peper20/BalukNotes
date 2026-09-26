// Команды приложения — одно место для палитры (Ctrl+K), горячих клавиш и
// справки (?). Новая возможность интерфейса = команда здесь.

import { app } from "./app.svelte";
import { noteHref, tagHref } from "./ids";
import { combo, type Combo } from "./keys";
import { mobile, ui } from "./ui.svelte";

export interface Command {
  id: string;
  title: string;
  group: string;
  /** Сочетания клавиш; первое показывается в палитре и справке. */
  keys?: Combo[];
  /** Доступна ли сейчас (например, только у открытой заметки). */
  available?: () => boolean;
  run: () => void;
}

const keys = (...specs: string[]) => specs.map(combo);
const hasNote = () => app.page != null;
const hasBook = () => ui.book != null;

/** Перейти к главе книги — через якорь её заголовка (попадает в историю). */
function chapter(delta: number) {
  const book = ui.book;
  const target = book?.chapters[ui.chapter + delta];
  if (target) app.go(`#${encodeURIComponent(target.id)}`);
}

function fontSize(delta: number) {
  const size = Number(app.settings["appearance.font_size"] ?? 19) + delta;
  void app.saveSettings({ "appearance.font_size": Math.min(Math.max(size, 12), 32) });
}

async function copyLink() {
  const id = app.currentId;
  if (!id) return;
  await navigator.clipboard?.writeText(`#см("${id}")`).catch(() => {});
  app.status = `скопировано: #см("${id}")`;
}

function toggleSidebar() {
  if (mobile.matches) ui.sidebarOpen = !ui.sidebarOpen;
  else ui.sidebarHidden = !ui.sidebarHidden;
}

/** § — на широком экране прячет/показывает боковое оглавление, на узком — всплывающее. */
export function toggleToc() {
  if (!app.toc.length) return;
  if (ui.tocRoom && !ui.tocOpen) void app.saveSettings({ "panels.toc": !app.settings["panels.toc"] });
  else ui.tocOpen = !ui.tocOpen;
}

/** Все команды; темы — по списку тем сервера. */
export function commands(): Command[] {
  const list: Command[] = [
    { id: "open", group: "Переход", title: "Быстрый переход к заметке", keys: keys("Ctrl+KeyO", "Ctrl+KeyP"), run: () => ui.openPalette("") },
    { id: "search", group: "Переход", title: "Поиск по тексту всех заметок", keys: keys("Ctrl+Shift+KeyF"), run: () => ui.openPalette("/") },
    { id: "commands", group: "Переход", title: "Команды", keys: keys("Ctrl+KeyK"), run: () => ui.openPalette(">") },
    { id: "home", group: "Переход", title: "Главная: граф и все заметки", keys: keys("KeyH"), run: () => app.go("/") },
    { id: "tags", group: "Переход", title: "Теги", run: () => app.go(tagHref()) },
    { id: "prev-chapter", group: "Переход", title: "Предыдущая глава", keys: keys("BracketLeft"), available: hasBook, run: () => chapter(-1) },
    { id: "next-chapter", group: "Переход", title: "Следующая глава", keys: keys("BracketRight"), available: hasBook, run: () => chapter(1) },

    { id: "new-tab", group: "Вкладки", title: "Новая вкладка", keys: keys("Alt+KeyT"), run: () => app.go("/", { newTab: true }) },
    { id: "close-tab", group: "Вкладки", title: "Закрыть вкладку", keys: keys("Alt+KeyW"), run: () => app.closeTab(app.activeTab) },
    { id: "next-tab", group: "Вкладки", title: "Следующая вкладка", keys: keys("Alt+BracketRight"), run: () => app.switchTab((app.activeTab + 1) % app.tabs.length) },
    { id: "prev-tab", group: "Вкладки", title: "Предыдущая вкладка", keys: keys("Alt+BracketLeft"), run: () => app.switchTab((app.activeTab - 1 + app.tabs.length) % app.tabs.length) },

    { id: "toc", group: "Вид", title: "Оглавление", keys: keys("KeyT"), available: () => app.toc.length > 0, run: toggleToc },
    { id: "reading", group: "Вид", title: "Режим чтения: только текст", keys: keys("KeyF"), run: () => (ui.reading = !ui.reading) },
    { id: "sidebar", group: "Вид", title: "Панель заметок", keys: keys("Ctrl+Backslash"), run: toggleSidebar },
    { id: "bigger", group: "Вид", title: "Кегль крупнее", keys: keys("Equal"), run: () => fontSize(1) },
    { id: "smaller", group: "Вид", title: "Кегль мельче", keys: keys("Minus"), run: () => fontSize(-1) },
    { id: "theme", group: "Вид", title: "Тема: следующая", run: () => app.cycleTheme() },
    { id: "theme-auto", group: "Вид", title: "Тема: как в системе", run: () => void app.saveSettings({ "appearance.theme": "auto" }) },
    ...app.themes.map((t) => ({
      id: `theme-${t.name}`,
      group: "Вид",
      title: `Тема: ${t.name}`,
      run: () => void app.saveSettings({ "appearance.theme": t.name }),
    })),

    { id: "refresh", group: "Заметка", title: "Пересобрать заметку", keys: keys("KeyR"), available: () => app.currentId != null, run: () => void app.check({ force: true }) },
    { id: "pdf", group: "Заметка", title: "PDF в текущей теме", available: hasNote, run: () => { const url = app.pdfUrl(); if (url) open(url, "_blank"); } },
    { id: "copy-link", group: "Заметка", title: "Скопировать ссылку #см(…) на заметку", available: () => app.currentId != null, run: () => void copyLink() },
    { id: "backlinks", group: "Заметка", title: "Кто ссылается сюда", available: hasNote, run: () => document.getElementById("backlinks")?.scrollIntoView({ behavior: "smooth" }) },

    { id: "settings", group: "Приложение", title: "Настройки", keys: keys("Ctrl+Comma"), run: () => (ui.settingsOpen = true) },
    { id: "help", group: "Приложение", title: "Горячие клавиши", keys: keys("?"), run: () => (ui.helpOpen = true) },
  ];
  for (let i = 1; i <= 9; i++) {
    list.push({ id: `tab-${i}`, group: "Вкладки", title: `Вкладка ${i}`, keys: keys(`Alt+Digit${i}`), available: () => app.tabs.length >= i, run: () => app.switchTab(i - 1) });
  }
  return list;
}

/** Открыть заметку из палитры или списка: Ctrl — в новой вкладке. */
export const openNote = (id: string, anchor: string | null, newTab: boolean) => app.go(noteHref(id, anchor), { newTab });
