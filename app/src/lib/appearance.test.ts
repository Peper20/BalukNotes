import { expect, it } from "vitest";
import { applyAppearance, resolveTheme } from "./appearance";

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

it("настройки — атрибутами на <html>", () => {
  const root = document.createElement("html");
  applyAppearance(root, { "appearance.font_size": 20, "appearance.measure": 38, "headings.numbering": "all", "header.tags": false, "panels.toc": true }, "night");
  expect(root.dataset.theme).toBe("night");
  expect(root.style.getPropertyValue("--k-size")).toBe("20px");
  expect(root.dataset.numbering).toBe("all");
  expect(root.dataset.headerTags).toBe("false");
  expect(root.dataset.toc).toBe("true");
});
