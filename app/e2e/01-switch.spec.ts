// The first build of a note: "визуализация" builds for seconds. The e2e
// server is warmed in advance (global-setup.ts), so the scenario appends a
// comment to the note file itself: the cache goes stale and the build is
// long again.
import { appendFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { VAULT, ready, vaultUrl } from "./helpers";

/** A note rebuild is not faster than the first build (dozens of figures in a debug build). */
const BUILD = { timeout: 60_000 };

/** Changes the note file without changing its look: the server cache goes stale. */
const touch = (file: string) => appendFileSync(join(VAULT, file), `\n// e2e: rebuild ${Date.now()}\n`);

test("navigating during a build: do not show a foreign note and do not jump back", async ({ page }) => {
  touch("демо/визуализация.typ");
  await page.goto(vaultUrl("/"));
  await ready(page);
  const tree = page.locator("#tree");
  const note = page.locator("#note");

  // The slow one (the first build, dozens of figures) -> at once the fast one.
  await tree.locator('a[data-id="демо/визуализация"]').click();
  await tree.locator('a[data-id="Сеть/UFW"]').click();
  await expect(note).toContainText("Порядок правил");

  // Back to the slow one while it may still be building: under its name -
  // the placeholder or the note itself, but not the UFW text.
  await tree.locator('a[data-id="демо/визуализация"]').click();
  await expect(page.locator("#crumbs")).toContainText("Визуализация");
  await expect(note).not.toContainText("Порядок правил");
  await expect(note.locator(".k-doc")).toContainText("Инструменты визуализации", BUILD);

  // The response to a cancelled request does not override the current note.
  await page.waitForTimeout(500);
  await expect(page.locator("#crumbs")).toContainText("Визуализация");
});

test("the \"собирается\" placeholder in place of the new note", async ({ page }) => {
  touch("ассессмент/01-тесты.typ");
  await page.goto(vaultUrl("/"));
  await ready(page);
  // The big chapter has not been built yet: first the placeholder with its name.
  await page.locator('#tree a[data-id="ассессмент/01-тесты"]').click();
  await expect(page.locator("#note .loading-name, #note .k-doc").first()).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("data-state", "ready", BUILD);
  await expect(page.locator("#note .loading")).toHaveCount(0);
  await expect(page.locator("#note .k-doc")).toContainText("Проектирование тестов");
});
