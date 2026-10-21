// Before the scenarios: builds the binaries once (the workers start them
// directly) and makes the template data of the app servers. The template is
// warmed: its server builds all notes, so the waits of the scenarios are short;
// every worker starts on a copy of it with the disk cache (e2e/fixtures.ts).
// Scenarios that need exactly the first build (01-switch) change the note file
// themselves, which makes its cache stale.
import { cpSync, mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import type { NoteListItem } from "../src/lib/api/types/NoteListItem";
import { VAULT_NAME } from "./helpers";
import { WARM_PORT } from "./ports";
import { E2E_DATA, ROOT, TEMPLATE_DATA, buildBinaries, exitOf, startApp, stop, waitFor } from "./server";

export default async function warmUp() {
  buildBinaries();
  rmSync(E2E_DATA, { recursive: true, force: true });
  mkdirSync(join(TEMPLATE_DATA, "vaults"), { recursive: true });
  cpSync(join(ROOT, "tests/vault"), join(TEMPLATE_DATA, "vaults", VAULT_NAME), { recursive: true, preserveTimestamps: true });

  const base = `http://127.0.0.1:${WARM_PORT}`;
  const server = startApp(TEMPLATE_DATA, WARM_PORT);
  try {
    await waitFor(`${base}/api/vaults`, 120_000, false, exitOf(server));
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
    console.log(`e2e template warmed: ${notes.length} notes in ${((Date.now() - started) / 1000).toFixed(1)} s`);
  } finally {
    // Stopped before the copies are made: the disk cache is complete.
    await stop(server);
  }
}
