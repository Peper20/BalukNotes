// View settings as attributes and variables on <html>; the rules are in
// baluk.css (the note look, src/baluk-css/) and app.css (the interface). The
// schema says how to apply a setting (`apply` of SettingDef, from
// notes-core::settings): a new setting = a schema entry + CSS, no changes here.

import type { SettingDef, SettingValues, Theme } from "./api";

/** The theme by the setting value: "auto" is the first theme of the needed lightness. */
export function resolveTheme(value: unknown, themes: Theme[], systemDark: boolean): string {
  if (value !== "auto" && themes.some((t) => t.name === value)) return value as string;
  return (themes.find((t) => t.dark === systemDark) ?? themes[0])?.name ?? "";
}

/**
 * The next theme for the button: always different **in look**. "Как в
 * системе" is not in the cycle - with it the first click often changed
 * nothing (the system already gave the same theme); that mode is in the settings.
 */
export function nextTheme(shown: string, themes: Theme[]): string {
  const i = themes.findIndex((t) => t.name === shown);
  return themes[(i + 1) % themes.length]?.name ?? shown;
}

/** What to remember for the first frame of the next load (`public/assets/theme.js`). */
export interface ThemeMemo {
  /** The chosen theme; null - "как в системе". */
  fixed: string | null;
  light: string;
  dark: string;
}

export function themeMemo(value: unknown, themes: Theme[]): ThemeMemo {
  const fixed = value !== "auto" && themes.some((t) => t.name === value) ? (value as string) : null;
  return { fixed, light: resolveTheme("auto", themes, false), dark: resolveTheme("auto", themes, true) };
}

/** Applies the view settings by the schema (`apply`) and the theme. */
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
