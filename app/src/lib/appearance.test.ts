import { expect, it } from "vitest";
import type { SettingDef } from "./api";
import { applyAppearance, nextTheme, resolveTheme } from "./appearance";

const themes = [
  { name: "classic", title: "Классика", dark: false },
  { name: "night", title: "Ночь", dark: true },
];

it("тема: явная, по системе, неизвестная", () => {
  expect(resolveTheme("night", themes, false)).toBe("night");
  expect(resolveTheme("auto", themes, true)).toBe("night");
  expect(resolveTheme("auto", themes, false)).toBe("classic");
  expect(resolveTheme("sepia", themes, true)).toBe("night");
});

it("кнопка темы: всегда другая на вид", () => {
  expect(nextTheme("night", themes)).toBe("classic");
  expect(nextTheme("classic", themes)).toBe("night");
  expect(nextTheme("sepia", themes)).toBe("classic");
});

const defs: SettingDef[] = [
  { key: "appearance.font_size", label: "", type: "number", min: 12, max: 32, step: 1, default: 19, apply: { to: "var", name: "--k-size", unit: "px" } },
  { key: "headings.numbering", label: "", type: "choice", options: [], default: "books", apply: { to: "attr", name: "data-numbering" } },
  { key: "header.tags", label: "", type: "bool", default: true, apply: { to: "attr", name: "data-header-tags" } },
  { key: "panels.toc", label: "", type: "bool", default: true, apply: { to: "attr", name: "data-toc" } },
  { key: "books.pages", label: "", type: "choice", options: [], default: "chapters" },
];

it("настройки — атрибутами и переменными на <html> по схеме", () => {
  const root = document.createElement("html");
  applyAppearance(root, defs, { "appearance.font_size": 20, "headings.numbering": "all", "header.tags": false, "books.pages": "whole" }, "night");
  expect(root.dataset.theme).toBe("night");
  expect(root.style.getPropertyValue("--k-size")).toBe("20px");
  expect(root.dataset.numbering).toBe("all");
  expect(root.dataset.headerTags).toBe("false");
  // нет значения — по умолчанию из схемы; без `apply` — не трогается
  expect(root.dataset.toc).toBe("true");
  expect(root.getAttributeNames().sort()).toEqual(["data-header-tags", "data-numbering", "data-theme", "data-toc", "style"]);
});

it("новая настройка вида — только записью в схеме (без правок TS)", () => {
  const root = document.createElement("html");
  const fresh: SettingDef = { key: "appearance.line", label: "", type: "number", min: 1, max: 2, step: 0.1, default: 1.5, apply: { to: "var", name: "--k-line-height", unit: "" } };
  applyAppearance(root, [...defs, fresh], {}, "classic");
  expect(root.style.getPropertyValue("--k-line-height")).toBe("1.5");
});
