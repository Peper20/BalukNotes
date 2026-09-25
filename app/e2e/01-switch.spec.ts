// Первым — пока кэш пуст и «визуализация» собирается секунды.
import { expect, test } from "@playwright/test";
import { ready } from "./helpers";

test("переход во время сборки: не показывать чужую заметку и не прыгать назад", async ({ page }) => {
  await page.goto("/");
  await ready(page);
  const tree = page.locator("#tree");
  const note = page.locator("#note");

  // Медленная (первая сборка, десятки рисунков) → сразу быстрая.
  await tree.getByRole("link", { name: "визуализация" }).click();
  await tree.getByRole("link", { name: "UFW" }).click();
  await expect(note).toContainText("Порядок правил");

  // Обратно к медленной, пока она, возможно, ещё собирается: под её именем —
  // заглушка или она сама, но не текст UFW.
  await tree.getByRole("link", { name: "визуализация" }).click();
  await expect(page.locator("#crumbs")).toContainText("визуализация");
  await expect(note).not.toContainText("Порядок правил");
  await expect(note.locator(".k-doc")).toContainText("Инструменты визуализации", { timeout: 60_000 });

  // Ответ на отменённый запрос не перебивает текущую заметку.
  await page.waitForTimeout(500);
  await expect(page.locator("#crumbs")).toContainText("визуализация");
});

test("заглушка «собирается» на месте новой заметки", async ({ page }) => {
  await page.goto("/");
  await ready(page);
  // Большая глава ещё не собиралась: сначала заглушка с её именем.
  await page.locator("#tree").getByRole("link", { name: "01-тесты" }).click();
  await expect(page.locator("#note .loading-name, #note .k-doc").first()).toBeVisible();
  await ready(page);
  await expect(page.locator("#note .loading")).toHaveCount(0);
  await expect(page.locator("#note .k-doc")).toContainText("Проектирование тестов");
});
