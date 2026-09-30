// Настройки вида — атрибутами и переменными на <html>; правила — в
// baluk.css (вид заметки, src/baluk-css/) и app.css (интерфейс). Как
// применить настройку, говорит схема (`apply` у SettingDef, из
// notes-core::settings): новая настройка = запись в схеме + CSS, без правок здесь.

import type { SettingDef, SettingValues, Theme } from "./api";

/** Тема по значению настройки: «auto» — первая тема нужной светлоты. */
export function resolveTheme(value: unknown, themes: Theme[], systemDark: boolean): string {
  if (value !== "auto" && themes.some((t) => t.name === value)) return value as string;
  return (themes.find((t) => t.dark === systemDark) ?? themes[0])?.name ?? "";
}

/**
 * Следующая тема для кнопки: всегда другая **на вид**. «Как в системе» в
 * цикл не входит — при нём первый клик часто ничего не менял (система уже
 * давала ту же тему); этот режим — в настройках.
 */
export function nextTheme(shown: string, themes: Theme[]): string {
  const i = themes.findIndex((t) => t.name === shown);
  return themes[(i + 1) % themes.length]?.name ?? shown;
}

/** Что запомнить для первого кадра следующей загрузки (`public/assets/theme.js`). */
export interface ThemeMemo {
  /** Выбранная тема; null — «как в системе». */
  fixed: string | null;
  light: string;
  dark: string;
}

export function themeMemo(value: unknown, themes: Theme[]): ThemeMemo {
  const fixed = value !== "auto" && themes.some((t) => t.name === value) ? (value as string) : null;
  return { fixed, light: resolveTheme("auto", themes, false), dark: resolveTheme("auto", themes, true) };
}

/** Применить настройки вида по схеме (`apply`) и тему. */
export function applyAppearance(root: HTMLElement, defs: readonly SettingDef[], v: SettingValues, theme: string): void {
  root.dataset.theme = theme;
  for (const def of defs) {
    const apply = def.apply;
    const value = v[def.key] ?? def.default;
    if (!apply) continue;
    if (apply.to === "attr") root.setAttribute(apply.name, String(value));
    else root.style.setProperty(apply.name, `${String(value)}${apply.unit}`);
  }
}
