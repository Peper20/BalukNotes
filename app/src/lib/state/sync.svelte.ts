// Vault sync of this device for the settings window (components/SyncSettings.svelte):
// the account, the vaults and what the core's workers do with them
// (`/api/device/sync`). No constant polling (project rule): the status is
// asked once when the window opens and again soon after an action, and then
// again every few seconds only while a vault is "syncing" (or a request of
// the user is still running). The window closed - nothing is asked.

import { api, ApiError, type SyncStatus } from "../api";
import { isBusy, actionMessage, loginMessage } from "../sync";

/** Pause between status requests while something is syncing, ms. */
const WHILE_SYNCING = 2000;
/** Status requests right after an action even if nothing shows "syncing" (the worker starts a round by itself). */
const AFTER_ACTION = 2;
/** The "N мин назад" texts are re-counted this often while the window is open, ms. */
const CLOCK = 30_000;

export type RowAction = "link" | "unlink" | "now" | "confirm" | "restore";

class Sync {
  status = $state.raw<SyncStatus | null>(null);
  /** The server runs without vault sync (404): the section says so and nothing else. */
  unavailable = $state(false);
  /** The status request failed (the app's own server did not answer). */
  failure = $state<string | null>(null);
  /** The first status has arrived (or failed). */
  loaded = $state(false);
  /** Unix seconds, for "синхронизировано N мин назад". */
  now = $state(Math.floor(Date.now() / 1000));
  loginBusy = $state(false);
  loginError = $state<string | null>(null);
  /** The answer of the last sign-in said: plain http to another computer. */
  insecure = $state(false);
  logoutError = $state<string | null>(null);
  /** What is being done to a vault now. */
  busy = $state.raw<Record<string, RowAction>>({});
  /** Why the last action on a vault failed. */
  rowError = $state.raw<Record<string, string>>({});

  #open = false;
  #seq = 0;
  #pending = 0;
  #follow = 0;
  #timer: ReturnType<typeof setTimeout> | undefined;
  #clock: ReturnType<typeof setInterval> | undefined;

  /** The settings window opened or closed. */
  watch(open: boolean): void {
    if (open === this.#open) return;
    this.#open = open;
    clearTimeout(this.#timer);
    clearInterval(this.#clock);
    if (!open) {
      this.#seq++;
      return;
    }
    this.loginError = this.logoutError = null;
    this.rowError = {};
    this.#clock = setInterval(() => (this.now = Math.floor(Date.now() / 1000)), CLOCK);
    void this.refresh();
  }

  /** Asks for the status; while the window is open and something goes on, schedules the next ask. */
  async refresh(): Promise<void> {
    const seq = ++this.#seq;
    clearTimeout(this.#timer);
    try {
      const status = await api.syncStatus();
      if (seq !== this.#seq) return;
      this.status = status;
      this.unavailable = false;
      this.failure = null;
    } catch (e) {
      if (seq !== this.#seq) return;
      if (e instanceof ApiError && e.status === 404) this.unavailable = true;
      else this.failure = (e as Error).message;
    }
    this.loaded = true;
    this.now = Math.floor(Date.now() / 1000);
    this.#schedule();
  }

  #schedule(): void {
    if (!this.#open || this.unavailable) return;
    const goes = this.#pending > 0 || isBusy(this.status?.vaults ?? []);
    if (!goes && this.#follow <= 0) return;
    if (!goes) this.#follow--;
    this.#timer = setTimeout(() => void this.refresh(), WHILE_SYNCING);
  }

  /** After an action: the status now, then a few more asks. */
  #acted(): Promise<void> {
    this.#follow = AFTER_ACTION;
    return this.refresh();
  }

  async login(server: string, login: string, password: string): Promise<boolean> {
    this.loginBusy = true;
    this.loginError = null;
    try {
      const account = await api.syncLogin({ server: server.trim(), login: login.trim(), password });
      this.insecure = account.insecure;
      await this.#acted();
      return true;
    } catch (e) {
      this.loginError = loginMessage(e);
      return false;
    } finally {
      this.loginBusy = false;
    }
  }

  async logout(): Promise<void> {
    this.logoutError = null;
    try {
      await api.syncLogout();
      this.insecure = false;
    } catch (e) {
      this.logoutError = actionMessage(e);
    }
    await this.#acted();
  }

  /** Turns the sync of a vault on (the first round runs inside) or off. */
  setLinked(name: string, linked: boolean): Promise<void> {
    return this.#row(name, linked ? "link" : "unlink", () => (linked ? api.syncLink(name) : api.syncUnlink(name)));
  }

  /** One round of a linked vault now. */
  syncNow(name: string): Promise<void> {
    return this.#row(name, "now", () => api.syncNow(name));
  }

  /** Goes on with the deletions a held round stopped at. */
  confirmDeletion(name: string): Promise<void> {
    return this.#row(name, "confirm", () => api.syncConfirm(name));
  }

  /** Gets the files a held round stopped at back from the storage server. */
  restoreFiles(name: string): Promise<void> {
    return this.#row(name, "restore", () => api.syncRestore(name));
  }

  async #row(name: string, action: RowAction, run: () => Promise<unknown>): Promise<void> {
    if (this.busy[name]) return;
    this.busy = { ...this.busy, [name]: action };
    this.rowError = Object.fromEntries(Object.entries(this.rowError).filter(([k]) => k !== name));
    this.#pending++;
    // Rounds take a while: while one runs, the rows show what the workers do.
    this.#schedule();
    try {
      await run();
    } catch (e) {
      this.rowError = { ...this.rowError, [name]: actionMessage(e) };
    } finally {
      this.#pending--;
      this.busy = Object.fromEntries(Object.entries(this.busy).filter(([k]) => k !== name));
    }
    await this.#acted();
  }
}

export const sync = new Sync();
