// Состояние интерфейса вокруг заметки: главы книги, оглавление, панели.

import type { Book } from "./book";

class Ui {
  /** Показанная книга по главам (DOM глав — вне страницы) или null. */
  book = $state.raw<Book | null>(null);
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
  /** Палитра: быстрый переход, команды (`>`), поиск (`/`), теги (`#`). */
  palette = $state<{ query: string } | null>(null);
  helpOpen = $state(false);
  /** Режим чтения: только текст — без панелей, вкладок и оглавления. */
  reading = $state(false);

  openPalette(query = ""): void {
    this.palette = { query };
  }
}

export const ui = new Ui();

export const mobile = matchMedia("(max-width: 800px)");
