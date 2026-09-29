// Состояние интерфейса вокруг заметки: главы книги, оглавление, панели.

import type { BookView } from "./api";
import { load, save } from "./storage";

class Ui {
  /** Показанная книга по главам (список глав и карта якорей — с сервера) или null. */
  book = $state.raw<BookView | null>(null);
  chapter = $state(0);
  /** id заголовка, который сейчас читают (подсветка в оглавлении). */
  currentHeading = $state<string | null>(null);
  /** Справа от колонки заметки хватает места для оглавления. */
  tocRoom = $state(false);
  tocWidth = $state(240);
  /** Всплывающее оглавление (узкий экран или скрытое боковое). */
  tocOpen = $state(false);
  /** Боковая панель: на телефоне — выезжает, на ПК — можно спрятать. */
  sidebarOpen = $state(false);
  sidebarHidden = $state(false);
  settingsOpen = $state(false);
  /**
   * Палитра: быстрый переход, команды (`>`), поиск (`/`), теги (`#`);
   * `book` — поиск в одной книге (её id).
   */
  palette = $state<{ query: string; book?: string | null } | null>(null);
  helpOpen = $state(false);
  /** Диалог «Новое хранилище». */
  vaultNewOpen = $state(false);
  /** Заметка, которую спрашивают, удалить ли (диалог подтверждения). */
  deleting = $state<string | null>(null);
  /** Меню заметки в дереве (правый клик, долгое касание): где и какой. */
  noteMenu = $state<{ id: string; x: number; y: number } | null>(null);
  /** Режим чтения: только текст — без панелей, вкладок и оглавления. */
  reading = $state(false);

  /** Свёрнутые папки дерева (пути) — запоминаются. */
  collapsed = $state<string[]>(load<string[]>("k-collapsed", []));

  setCollapsed(path: string, closed: boolean): void {
    const has = this.collapsed.includes(path);
    if (closed === has) return;
    this.collapsed = closed ? [...this.collapsed, path] : this.collapsed.filter((p) => p !== path);
    save("k-collapsed", this.collapsed);
  }

  openPalette(query = "", book: string | null = null): void {
    this.palette = { query, book };
  }
}

export const ui = new Ui();

export const mobile = matchMedia("(max-width: 800px)");
