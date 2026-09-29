// Прогрев сервера e2e до сценариев: все заметки собираются заранее, чтобы
// сценарий не ждал первую сборку (в отладочной сборке книга — секунды) и
// ожидания не упирались в тайм-аут. Сценарии, которым нужна именно первая
// сборка (01-switch), сами меняют файл заметки — её кэш устаревает.
import type { FullConfig } from "@playwright/test";
import type { NoteListItem } from "../src/lib/api/types/NoteListItem";
import { VAULT_NAME } from "./helpers";

export default async function warmUp(config: FullConfig) {
  const base = config.projects[0]?.use.baseURL;
  if (!base) throw new Error("нет baseURL в playwright.config.ts");
  const started = Date.now();
  const api = `${base}/api/vaults/${VAULT_NAME}`;
  const res = await fetch(`${api}/notes`);
  if (!res.ok) throw new Error(`прогрев: список заметок — ${res.status}`);
  const notes = (await res.json()) as NoteListItem[];
  // По одной: сервер всё равно собирает заметки по очереди.
  for (const { id } of notes) {
    const path = id.split("/").map(encodeURIComponent).join("/");
    const note = await fetch(`${api}/notes/${path}`);
    if (!note.ok) throw new Error(`прогрев: ${id} — ${note.status}`);
    await note.arrayBuffer();
  }
  console.log(`прогрев сервера e2e: ${notes.length} заметок за ${((Date.now() - started) / 1000).toFixed(1)} с`);
}
