// Settings (schema and values from the server) and themes. The look is
// applied by attributes on <html> (appearance.ts); whoever depends on
// settings changes subscribes with `onSaved`.
//
// In a vault the values are the ones shared by all vaults (`shared`) and on
// top of them those set only in it (`own`). A change from the interface is
// for the shown vault; for `shared` settings (theme, font size) and device
// ones it is for all (user's decisions), unless the vault has its own value
// - then for it. "For all vaults" is `forAll`, "only here" is `onlyHere`.
// Without a vault (the picker screen), only the shared ones.

import { api, type Schema, type SettingValues, type Theme, type VaultSettingsResponse } from "../api";
import { applyAppearance, nextTheme, resolveTheme, themeMemo } from "../appearance";
import { save, saveShared } from "../storage";
import { vault } from "../vault";

type Value = SettingValues[string];

/** Where a setting value comes from: the default, shared by all vaults (changed) or only this vault's. */
export type SettingSource = "default" | "shared" | "own";


class Settings {
  values = $state.raw<SettingValues>({});
  /** Shared by all vaults. */
  shared = $state.raw<SettingValues>({});
  /** Set only in the shown vault. */
  own = $state.raw<SettingValues>({});
  schema = $state.raw<Schema | null>(null);
  themes = $state.raw<Theme[]>([]);
  systemDark = $state(false);
  /** The server did not accept the last change. */
  error = $state<string | null>(null);

  /** The shown theme: "auto" follows the system. */
  theme = $derived(resolveTheme(this.values["appearance.theme"], this.themes, this.systemDark));

  #saved: ((keys: string[]) => void)[] = [];

  async load(): Promise<void> {
    const dark = matchMedia("(prefers-color-scheme: dark)");
    this.systemDark = dark.matches;
    dark.addEventListener("change", () => (this.systemDark = dark.matches));
    if (vault() != null) {
      const [settings, themes] = await Promise.all([api.vaultSettings(), api.themes()]);
      this.#take(settings);
      this.themes = themes;
    } else {
      const [settings, themes] = await Promise.all([api.settings(), api.themes()]);
      this.#take({ schema: settings.schema, values: settings.values, shared: settings.values, own: {} });
      this.themes = themes;
    }
  }

  #take(r: VaultSettingsResponse): void {
    this.schema = r.schema;
    this.shared = r.shared;
    this.own = r.own;
    this.values = r.values;
  }

  #shared(values: SettingValues): void {
    this.shared = values;
    this.values = { ...values, ...this.own };
  }

  /** Where the value of the setting `key` comes from. */
  source(key: string): SettingSource {
    if (key in this.own) return "own";
    const def = this.schema?.settings.find((s) => s.key === key);
    return def && this.shared[key] !== def.default ? "shared" : "default";
  }

  /** Calls `fn` with the keys after every save. */
  onSaved(fn: (keys: string[]) => void): void {
    this.#saved.push(fn);
  }

  async #run(keys: string[], action: () => Promise<void>): Promise<void> {
    try {
      await action();
      this.error = null;
      for (const fn of this.#saved) fn(keys);
    } catch (e) {
      this.error = (e as Error).message;
    }
  }

  /** A change of `key` is shared by all vaults (otherwise only for the shown one). */
  #everywhere(key: string): boolean {
    if (vault() == null) return true;
    if (key in this.own) return false;
    const def = this.schema?.settings.find((s) => s.key === key);
    return Boolean(def?.device || def?.shared);
  }

  /** Changes it for the shown vault or for all (`#everywhere`). */
  async save(patch: SettingValues): Promise<void> {
    const shared = Object.fromEntries(Object.entries(patch).filter(([k]) => this.#everywhere(k)));
    const own = Object.fromEntries(Object.entries(patch).filter(([k]) => !this.#everywhere(k)));
    await this.#run(Object.keys(patch), async () => {
      if (Object.keys(shared).length) this.#shared(await api.saveSettings(shared));
      if (Object.keys(own).length) this.#take(await api.saveVaultSettings(own));
    });
  }

  /** This vault's value becomes shared by all; the vault takes the shared one again. */
  async forAll(key: string): Promise<void> {
    const value: Value | undefined = this.own[key];
    if (value === undefined) return;
    await this.#run([key], async () => {
      this.#shared(await api.saveSettings({ [key]: value }));
      this.#take(await api.saveVaultSettings({ [key]: null }));
    });
  }

  /** The current value becomes the vault's own: further changes are only for it. */
  async onlyHere(key: string): Promise<void> {
    const value: Value | undefined = this.values[key];
    if (value === undefined || vault() == null) return;
    await this.#run([key], async () => this.#take(await api.saveVaultSettings({ [key]: value })));
  }

  /** Removes the vault's own value: shared again. */
  async useShared(key: string): Promise<void> {
    await this.#run([key], async () => this.#take(await api.saveVaultSettings({ [key]: null })));
  }

  /** The shared value goes back to the default (in all vaults where it is not their own). */
  async resetShared(key: string): Promise<void> {
    const def = this.schema?.settings.find((s) => s.key === key);
    if (!def) return;
    await this.#run([key], async () => this.#shared(await api.saveSettings({ [key]: def.default as Value })));
  }

  cycleTheme(): void {
    const patch = { "appearance.theme": nextTheme(this.theme, this.themes) };
    // At once, without waiting for the server: otherwise the click feels ignored.
    this.values = { ...this.values, ...patch };
    void this.save(patch);
  }

  /** The look after the settings load: before that <html> has the theme from `public/assets/theme.js`. */
  apply(root: HTMLElement): void {
    if (!this.schema || !this.themes.length) return;
    applyAppearance(root, this.schema.settings, this.values, this.theme);
    // The theme also for the first frame of the next load: of this vault and
    // of any vault that has not remembered it yet.
    const memo = themeMemo(this.values["appearance.theme"], this.themes);
    save("k-theme", memo);
    saveShared("k-theme", memo);
  }
}

export const settings = new Settings();
