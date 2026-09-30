// Настройки (схема и значения — с сервера) и темы. Вид применяется
// атрибутами на <html> (appearance.ts); кто зависит от смены настроек —
// подписывается `onSaved`.
//
// В хранилище значения — общие для всех хранилищ (`shared`) и поверх них
// заданные только в нём (`own`). Изменение из интерфейса — для показанного
// хранилища; у настроек `shared` (тема, кегль) и устройства — для всех
// (решения пользователя), а если у хранилища своё значение — для него.
// «Для всех хранилищ» — `forAll`, «только здесь» — `onlyHere`. Без
// хранилища (экран выбора) — только общие.

import { api, type Schema, type SettingValues, type Theme, type VaultSettingsResponse } from "../api";
import { applyAppearance, nextTheme, resolveTheme } from "../appearance";
import { vault } from "../vault";

type Value = SettingValues[string];

/** Откуда значение настройки: по умолчанию, общее для всех хранилищ (изменено) или только этого хранилища. */
export type SettingSource = "default" | "shared" | "own";


class Settings {
  values = $state.raw<SettingValues>({});
  /** Общие для всех хранилищ. */
  shared = $state.raw<SettingValues>({});
  /** Заданные только в показанном хранилище. */
  own = $state.raw<SettingValues>({});
  schema = $state.raw<Schema | null>(null);
  themes = $state.raw<Theme[]>([]);
  systemDark = $state(false);
  /** Сервер не принял последнее изменение. */
  error = $state<string | null>(null);

  /** Показанная тема: «auto» — по системе. */
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

  /** Откуда значение настройки `key`. */
  source(key: string): SettingSource {
    if (key in this.own) return "own";
    const def = this.schema?.settings.find((s) => s.key === key);
    return def && this.shared[key] !== def.default ? "shared" : "default";
  }

  /** Вызвать `fn` с ключами после каждого сохранения. */
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

  /** Изменение `key` — общее для всех хранилищ (иначе — только для показанного). */
  #everywhere(key: string): boolean {
    if (vault() == null) return true;
    if (key in this.own) return false;
    const def = this.schema?.settings.find((s) => s.key === key);
    return Boolean(def?.device || def?.shared);
  }

  /** Изменить — для показанного хранилища или для всех (`#everywhere`). */
  async save(patch: SettingValues): Promise<void> {
    const shared = Object.fromEntries(Object.entries(patch).filter(([k]) => this.#everywhere(k)));
    const own = Object.fromEntries(Object.entries(patch).filter(([k]) => !this.#everywhere(k)));
    await this.#run(Object.keys(patch), async () => {
      if (Object.keys(shared).length) this.#shared(await api.saveSettings(shared));
      if (Object.keys(own).length) this.#take(await api.saveVaultSettings(own));
    });
  }

  /** Значение этого хранилища — общим для всех; у хранилища — снова общее. */
  async forAll(key: string): Promise<void> {
    const value: Value | undefined = this.own[key];
    if (value === undefined) return;
    await this.#run([key], async () => {
      this.#shared(await api.saveSettings({ [key]: value }));
      this.#take(await api.saveVaultSettings({ [key]: null }));
    });
  }

  /** Нынешнее значение — своим у хранилища: дальше изменения — только для него. */
  async onlyHere(key: string): Promise<void> {
    const value: Value | undefined = this.values[key];
    if (value === undefined || vault() == null) return;
    await this.#run([key], async () => this.#take(await api.saveVaultSettings({ [key]: value })));
  }

  /** Убрать своё значение хранилища: снова общее. */
  async useShared(key: string): Promise<void> {
    await this.#run([key], async () => this.#take(await api.saveVaultSettings({ [key]: null })));
  }

  /** Общее значение — снова по умолчанию (у всех хранилищ, где оно не своё). */
  async resetShared(key: string): Promise<void> {
    const def = this.schema?.settings.find((s) => s.key === key);
    if (!def) return;
    await this.#run([key], async () => this.#shared(await api.saveSettings({ [key]: def.default as Value })));
  }

  cycleTheme(): void {
    const patch = { "appearance.theme": nextTheme(this.theme, this.themes) };
    // Сразу, не дожидаясь сервера: иначе клик кажется непринятым.
    this.values = { ...this.values, ...patch };
    void this.save(patch);
  }

  apply(root: HTMLElement): void {
    applyAppearance(root, this.schema?.settings ?? [], this.values, this.theme);
  }
}

export const settings = new Settings();
