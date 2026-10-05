// Warms the e2e server before the scenarios: all notes are built in advance
// so that a scenario does not wait for the first build (a book takes seconds
// in a debug build) and the waits do not hit the timeout. Scenarios that
// need exactly the first build (01-switch) change the note file themselves,
// which makes its cache stale.
import type { FullConfig } from "@playwright/test";
import type { NoteListItem } from "../src/lib/api/types/NoteListItem";
import { VAULT_NAME } from "./helpers";

export default async function warmUp(config: FullConfig) {
  const base = config.projects[0]?.use.baseURL;
  if (!base) throw new Error("no baseURL in playwright.config.ts");
  const started = Date.now();
  const api = `${base}/api/vaults/${VAULT_NAME}`;
  const res = await fetch(`${api}/notes`);
  if (!res.ok) throw new Error(`warming: the note list - ${res.status}`);
  const notes = (await res.json()) as NoteListItem[];
  // One by one: the server builds notes in turn anyway.
  for (const { id } of notes) {
    const path = id.split("/").map(encodeURIComponent).join("/");
    const note = await fetch(`${api}/notes/${path}`);
    if (!note.ok) throw new Error(`warming: ${id} - ${note.status}`);
    await note.arrayBuffer();
  }
  console.log(`e2e server warmed: ${notes.length} notes in ${((Date.now() - started) / 1000).toFixed(1)} s`);
}
