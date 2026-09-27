// Настройки (схема и значения — с сервера) и темы. Вид применяется
// атрибутами на <html> (appearance.ts); кто зависит от смены настроек —
// подписывается `onSaved`.

import { api, type Schema, type SettingValues, type Theme } from "../api";
import { applyAppearance, nextTheme, resolveTheme } from "../appearance";

class Settings {
  values = $state.raw<SettingValues>({});
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
    const [settings, themes] = await Promise.all([api.settings(), api.themes()]);
    this.schema = settings.schema;
    this.values = settings.values;
    this.themes = themes;
  }

  /** Вызвать `fn` с ключами после каждого сохранения. */
  onSaved(fn: (keys: string[]) => void): void {
    this.#saved.push(fn);
  }

  async save(patch: SettingValues): Promise<void> {
    try {
      this.values = await api.saveSettings(patch);
      this.error = null;
      const keys = Object.keys(patch);
      for (const fn of this.#saved) fn(keys);
    } catch (e) {
      this.error = (e as Error).message;
    }
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
