// Список заметок и папок хранилища. Заметки создаёт Claude Code в любой
// момент — список обновляется при каждой проверке, без перезагрузки страницы.
// Показываются названия (из файла заметки и `_folder.toml` папки), а не
// имена файлов.

import { api, type FolderListItem, type NoteListItem } from "../api";
import { splitId } from "../ids";

class Notes {
  all = $state.raw<NoteListItem[]>([]);
  folders = $state.raw<FolderListItem[]>([]);
  #folderTitles = $derived(new Map(this.folders.map((f) => [f.path, f.title])));

  byId(id: string | null): NoteListItem | undefined {
    return id == null ? undefined : this.all.find((n) => n.id === id);
  }

  /** Название заметки; нет в списке (ещё не написана) — имя из пути. */
  title(id: string): string {
    return this.byId(id)?.title ?? splitId(id).name;
  }

  /** Название папки; нет в списке — её имя. */
  folderTitle(path: string): string {
    return this.#folderTitles.get(path) ?? path.slice(path.lastIndexOf("/") + 1);
  }

  /** Путь папки названиями: `Учёба / Матан`; корень — пусто. */
  folderLabel(path: string): string {
    if (!path) return "";
    const parts = path.split("/");
    return parts.map((_, i) => this.folderTitle(parts.slice(0, i + 1).join("/"))).join(" / ");
  }

  /** Заметки и папки с сервера (при запуске). */
  async load(): Promise<void> {
    const [all, folders] = await Promise.all([api.notes(), api.folders()]);
    this.all = all;
    this.folders = folders;
  }

  async refresh(): Promise<void> {
    try {
      const [all, folders] = await Promise.all([api.notes(), api.folders()]);
      if (JSON.stringify(all) !== JSON.stringify(this.all)) this.all = all;
      if (JSON.stringify(folders) !== JSON.stringify(this.folders)) this.folders = folders;
    } catch {
      // сервер недоступен — попробуем в следующий раз
    }
  }
}

export const notes = new Notes();
