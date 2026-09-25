import { expect, it } from "vitest";
import { applyAppearance, resolveTheme } from "./appearance";

const themes = [
  { name: "классика", dark: false },
  { name: "ночь", dark: true },
];

it("тема: явная, по системе, неизвестная", () => {
  expect(resolveTheme("ночь", themes, false)).toBe("ночь");
  expect(resolveTheme("auto", themes, true)).toBe("ночь");
  expect(resolveTheme("auto", themes, false)).toBe("классика");
  expect(resolveTheme("сепия", themes, true)).toBe("ночь");
});

it("настройки — атрибутами на <html>", () => {
  const root = document.createElement("html");
  applyAppearance(root, { "appearance.font_size": 20, "appearance.measure": 38, "headings.numbering": "all", "header.tags": false, "panels.toc": true }, "ночь");
  expect(root.dataset.theme).toBe("ночь");
  expect(root.style.getPropertyValue("--k-size")).toBe("20px");
  expect(root.dataset.numbering).toBe("all");
  expect(root.dataset.headerTags).toBe("false");
  expect(root.dataset.toc).toBe("true");
});
