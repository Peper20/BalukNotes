// Переименование заметки или папки — что переезжает вслед за ней в клиенте
// (вкладки, места чтения, свёрнутые папки). Файлы меняет сервер.

/** Новый путь `id` после переименования `from` → `to` (и всего, что внутри `from`); не затронут — null. */
export function movedId(id: string, from: string, to: string): string | null {
  if (id === from) return to;
  return id.startsWith(`${from}/`) ? to + id.slice(from.length) : null;
}
