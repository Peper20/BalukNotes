// Первая сборка заметки: «визуализация» собирается секунды. Сервер e2e
// прогрет заранее (global-setup.ts), поэтому сценарий сам дописывает в файл
// заметки комментарий — кэш устаревает, и сборка снова долгая.
import { appendFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { VAULT, ready, vaultUrl } from "./helpers";

/** Пересборка заметки не быстрее первой (десятки рисунков в отладочной сборке). */
const BUILD = { timeout: 60_000 };

/** Изменить файл заметки, не меняя её вида: кэш сервера устаревает. */
const touch = (file: string) => appendFileSync(join(VAULT, file), `\n// e2e: пересобрать ${Date.now()}\n`);

test("переход во время сборки: не показывать чужую заметку и не прыгать назад", async ({ page }) => {
  touch("демо/визуализация.typ");
  await page.goto(vaultUrl("/"));
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
  await expect(note.locator(".k-doc")).toContainText("Инструменты визуализации", BUILD);

  // Ответ на отменённый запрос не перебивает текущую заметку.
  await page.waitForTimeout(500);
  await expect(page.locator("#crumbs")).toContainText("визуализация");
});

test("заглушка «собирается» на месте новой заметки", async ({ page }) => {
  touch("ассессмент/01-тесты.typ");
  await page.goto(vaultUrl("/"));
  await ready(page);
  // Большая глава ещё не собиралась: сначала заглушка с её именем.
  await page.locator("#tree").getByRole("link", { name: "01-тесты" }).click();
  await expect(page.locator("#note .loading-name, #note .k-doc").first()).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("data-state", "ready", BUILD);
  await expect(page.locator("#note .loading")).toHaveCount(0);
  await expect(page.locator("#note .k-doc")).toContainText("Проектирование тестов");
});
