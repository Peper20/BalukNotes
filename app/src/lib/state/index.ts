// Client state: store modules with narrow links:
//
// - settings: settings and themes;
// - notes: the note list;
// - tabs, places: tabs, reading places and recent notes (localStorage);
// - router: page address -> what to show, navigation;
// - reader: the shown note - loading, status, scroll;
// - updates: the change check;
// - connection: the connection to the server ("нет связи" and back).
//
// Pure logic is in ../tabs.ts, ../places.ts, ../reading.ts (with Vitest).
// The interface around a note (panels, chapter, outline) is ../ui.svelte.ts.

import { api, rebaseStylesheets } from "../api";
import { rememberVault } from "../boot";
import { parseRoute } from "../ids";
import { connection } from "./connection.svelte";
import { notes } from "./notes.svelte";
import { places } from "./places.svelte";
import { reader } from "./reader.svelte";
import { router } from "./router.svelte";
import { settings } from "./settings.svelte";
import { tabs } from "./tabs.svelte";
import { updates } from "./updates.svelte";

export { connection, notes, places, reader, router, settings, tabs, updates };

/** Client start: settings and the note list from the server, the route, warming. */
export async function start(): Promise<void> {
  rebaseStylesheets();
  connection.start();
  // The note list after the settings: the look (font size) changes the width
  // of tab titles, and the tab bar scrolls to the active tab by the note list.
  await Promise.all([settings.load(), notes.load()]);
  rememberVault();
  updates.start();
  history.scrollRestoration = "manual";
  tabs.restore();
  router.sync();
  // The server builds all notes in advance, first those in tabs and recent ones.
  const tabIds = tabs.list.map((t) => parseRoute(new URL(t.url, location.href).pathname)).flatMap((r) => (r.kind === "note" ? [r.id] : []));
  void api.warm({ ids: [...new Set([...tabIds, ...places.recent])] }).catch(() => {});
  addEventListener("popstate", () => router.sync({ pop: true }));
  addEventListener("pagehide", () => reader.remember());
}
