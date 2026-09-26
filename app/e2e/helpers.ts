import { fileURLToPath } from "node:url";
import { expect, type Page } from "@playwright/test";

/** Хранилище сервера e2e — копия tests/vault (см. playwright.config.ts). */
export const VAULT = fileURLToPath(new URL("../../tests/.data/e2e/vault/", import.meta.url));

/** Адрес заметки, как его строит клиент. */
export const noteUrl = (id: string, anchor?: string) =>
  `/n/${id.split("/").map(encodeURIComponent).join("/")}${anchor ? `#${encodeURIComponent(anchor)}` : ""}`;

/** Клиент дорисовал заметку или главную (<html data-state="ready">). */
export async function ready(page: Page) {
  await expect(page.locator("html")).toHaveAttribute("data-state", "ready");
}

export async function open(page: Page, id: string, anchor?: string) {
  await page.goto(noteUrl(id, anchor));
  await ready(page);
}

/** Заголовок заметки (h1 титула). */
export const title = (page: Page) => page.locator("#note .k-title h1");

/** Настройки общие для всех сценариев: вернуть тему «как в системе». */
export async function resetTheme(page: Page) {
  const res = await page.request.put("/api/settings", { data: { "appearance.theme": "auto" } });
  if (!res.ok()) throw new Error(`тема не сброшена: ${res.status()}`);
}
