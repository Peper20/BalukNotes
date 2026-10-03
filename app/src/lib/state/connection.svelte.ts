// Связь с сервером. Пропала — запрос не дошёл (в том числе ждущий запрос событий):
// интерфейс показывает «нет связи» (метка в Topbar), показанная заметка
// остаётся на месте. Пока связи нет — пробный запрос раз в `PROBE_MS`
// (это проверка связи, а не изменений: опроса изменений нет); связь вернулась —
// подписчики `onBack` проверяют изменения и догружают то, что не открылось.

import { api, onReach } from "../api";

/** Как часто пробовать связь, пока её нет. */
export const PROBE_MS = 3000;

class Connection {
  online = $state(true);
  #timer: ReturnType<typeof setTimeout> | undefined;
  #back: (() => void)[] = [];

  start(): void {
    onReach((ok) => (ok ? this.reached() : this.lost()));
  }

  /** Вызвать `fn`, когда связь вернётся. */
  onBack(fn: () => void): void {
    this.#back.push(fn);
  }

  lost(): void {
    if (!this.online) return;
    this.online = false;
    this.#probe();
  }

  reached(): void {
    if (this.online) return;
    this.online = true;
    clearTimeout(this.#timer);
    for (const fn of this.#back) fn();
  }

  /** Попробовать сейчас (кнопка «Повторить»). */
  retry(): void {
    clearTimeout(this.#timer);
    void api.vaults().catch(() => this.#probe());
  }

  #probe(): void {
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => this.retry(), PROBE_MS);
  }
}

export const connection = new Connection();
