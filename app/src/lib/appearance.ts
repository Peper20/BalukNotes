// Настройки вида — атрибутами и переменными на <html>; правила — в
// baluk.css (вид заметки) и app.css (интерфейс). Новая настройка =
// запись в notes-core::settings::Schema + строка здесь + CSS.

import type { SettingValues, Theme } from "./api";

/** Тема по значению настройки: «auto» — первая тема нужной светлоты. */
export function resolveTheme(value: unknown, themes: Theme[], systemDark: boolean): string {
  if (value !== "auto" && themes.some((t) => t.name === value)) return value as string;
  return (themes.find((t) => t.dark === systemDark) ?? themes[0])?.name ?? "";
}

export function applyAppearance(root: HTMLElement, v: SettingValues, theme: string): void {
  root.dataset.theme = theme;
  root.style.setProperty("--k-size", `${v["appearance.font_size"]}px`);
  root.style.setProperty("--k-measure", `${v["appearance.measure"]}em`);
  root.dataset.numbering = String(v["headings.numbering"]);
  root.dataset.chapters = String(v["headings.chapters"]);
  for (const part of ["title", "kind", "description", "byline", "tags"]) {
    root.dataset[`header${part[0]!.toUpperCase()}${part.slice(1)}`] = String(v[`header.${part}`]);
  }
  root.dataset.toc = String(v["panels.toc"]);
  root.dataset.backlinks = String(v["panels.backlinks"]);
}
