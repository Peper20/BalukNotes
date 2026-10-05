// The connection to the server. Lost - a request did not get through (the
// waiting events request too): the interface shows "нет связи" (a label in
// Topbar), the shown note stays. While there is no connection, a probe
// request every `PROBE_MS` (a connection check, not a change check: there is
// no change polling); back - the `onBack` subscribers check for changes and
// load what did not open.

import { api, onReach } from "../api";

/** How often to probe the connection while it is lost. */
export const PROBE_MS = 3000;

class Connection {
  online = $state(true);
  #timer: ReturnType<typeof setTimeout> | undefined;
  #back: (() => void)[] = [];

  start(): void {
    onReach((ok) => (ok ? this.reached() : this.lost()));
  }

  /** Calls `fn` when the connection is back. */
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

  /** Tries now (the "Повторить" button). */
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
