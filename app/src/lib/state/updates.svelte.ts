// Обновление: изменились файлы показанной заметки — перезагрузить её.
// Проверка — сверка версии (/api/version/… — дёшево: сервер ничего не
// компилирует, если файлы не менялись) по кнопке, при возврате в окно и по
// сигналу источника изменений (../changes.ts: опрос раз в N секунд).

import { api } from "../api";
import { polling, type ChangeSource } from "../changes";
import { notes } from "./notes.svelte";
import { reader } from "./reader.svelte";
import { router } from "./router.svelte";
import { settings } from "./settings.svelte";

class Updates {
  #stop = () => {};

  start(): void {
    this.#schedule();
    settings.onSaved((keys) => this.#settingsSaved(keys));
    addEventListener("focus", () => settings.values["refresh.on_focus"] && this.check());
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
      // сервер недоступен — попробуем в следующий раз
    }
  }

  /** Источник изменений по настройкам. */
  #source(): ChangeSource {
    return polling(Number(settings.values["refresh.interval"]));
  }

  #schedule(): void {
    this.#stop();
    this.#stop = this.#source().start(() => void this.check());
  }

  #settingsSaved(keys: string[]): void {
    this.#schedule();
    // Настройки отрисовки (figures.*) меняют версию страницы на сервере,
    // вид книги (books.*) — раскладку уже полученной страницы.
    if (keys.some((k) => k.startsWith("figures."))) void this.check();
    else if (keys.some((k) => k.startsWith("books."))) void reader.reload();
  }
}

export const updates = new Updates();
