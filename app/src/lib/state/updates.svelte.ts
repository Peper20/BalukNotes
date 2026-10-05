// Updates: the files of the shown note changed - reload it. The check is a
// version comparison (.../version/... - cheap: the server compiles nothing
// if the files did not change) on the "Обновить" button and, if the setting
// `refresh.mode` is "automatically", on returning to the window and on a
// server event (../changes.ts). The connection is back - a check (and a note
// that did not open without the connection gets loaded).

import { api } from "../api";
import { changeSource, type RefreshMode } from "../changes";
import { connection } from "./connection.svelte";
import { notes } from "./notes.svelte";
import { reader } from "./reader.svelte";
import { router } from "./router.svelte";
import { settings } from "./settings.svelte";

class Updates {
  #stop = () => {};

  start(): void {
    this.#schedule();
    settings.onSaved((keys) => this.#settingsSaved(keys));
    addEventListener("focus", () => this.#mode() === "auto" && this.check());
    connection.onBack(() => void (reader.failure ? reader.reload() : this.check()));
  }

  #mode(): RefreshMode {
    return settings.values["refresh.mode"] === "manual" ? "manual" : "auto";
  }

  /** Whether the note files changed, and if so, reloads it. */
  async check({ force = false } = {}): Promise<void> {
    void notes.refresh();
    const id = router.currentId;
    if (!id || reader.pending) return;
    if (force) return reader.reload();
    try {
      const { version } = await api.version(id);
      // Another note may have been opened while waiting for the response.
      if (router.currentId === id && !reader.pending && version !== reader.version) await reader.reload();
    } catch {
      // the server is unreachable: we will check when the connection is back
    }
  }

  #schedule(): void {
    this.#stop();
    this.#stop = changeSource(this.#mode(), api.events).start(() => void this.check());
  }

  #settingsSaved(keys: string[]): void {
    if (keys.includes("refresh.mode")) this.#schedule();
    // Rendering settings (figures.*) change the page version on the server,
    // the book look (books.*) the layout of an already received page.
    if (keys.some((k) => k.startsWith("figures."))) void this.check();
    else if (keys.some((k) => k.startsWith("books."))) void reader.reload();
  }
}

export const updates = new Updates();
