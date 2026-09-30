// Обновление: изменились файлы показанной заметки — перезагрузить её.
// Проверка — сверка версии (…/version/… — дёшево: сервер ничего не
// компилирует, если файлы не менялись) по кнопке «Обновить» и, если
// настройка `refresh.mode` — «автоматически», при возврате в окно и по
// событию сервера (../changes.ts). Связь вернулась — проверка (а заметку,
// которая не открылась без связи, — загрузить).

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

  /** Изменились ли файлы заметки — и если да, перезагрузить её. */
  async check({ force = false } = {}): Promise<void> {
    void notes.refresh();
    const id = router.currentId;
    if (!id || reader.pending) return;
    if (force) return reader.reload();
    try {
      const { version } = await api.version(id);
      // Пока ждали ответ, могли перейти на другую заметку.
      if (router.currentId === id && !reader.pending && version !== reader.version) await reader.reload();
    } catch {
      // сервер недоступен — проверим, когда связь вернётся
    }
  }

  #schedule(): void {
    this.#stop();
    const reach = (ok: boolean) => (ok ? connection.reached() : connection.lost());
    this.#stop = changeSource(this.#mode(), api.eventsUrl(), reach).start(() => void this.check());
  }

  #settingsSaved(keys: string[]): void {
    if (keys.includes("refresh.mode")) this.#schedule();
    // Настройки отрисовки (figures.*) меняют версию страницы на сервере,
    // вид книги (books.*) — раскладку уже полученной страницы.
    if (keys.some((k) => k.startsWith("figures."))) void this.check();
    else if (keys.some((k) => k.startsWith("books."))) void reader.reload();
  }
}

export const updates = new Updates();
