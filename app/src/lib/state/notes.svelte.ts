// Список заметок хранилища. Заметки создаёт Claude Code в любой момент —
// список обновляется при каждой проверке, без перезагрузки страницы.

import { api, type NoteListItem } from "../api";

class Notes {
  all = $state.raw<NoteListItem[]>([]);

  byId(id: string | null): NoteListItem | undefined {
    return id == null ? undefined : this.all.find((n) => n.id === id);
  }

  async refresh(): Promise<void> {
    try {
      const all = await api.notes();
      if (JSON.stringify(all) !== JSON.stringify(this.all)) this.all = all;
    } catch {
      // сервер недоступен — попробуем в следующий раз
    }
  }
}

export const notes = new Notes();
