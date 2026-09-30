import { fileURLToPath } from "node:url";
import { expect, type Page } from "@playwright/test";

/** Хранилище сервера e2e — копия tests/vault (см. playwright.config.ts). */
export const VAULT_NAME = "vault";
export const VAULT = fileURLToPath(new URL(`../../tests/.data/e2e/vaults/${VAULT_NAME}/`, import.meta.url));
/** Корзина сервера e2e: удалённые заметки — сюда, а не в корзину системы. */
export const TRASH = fileURLToPath(new URL("../../tests/.data/e2e/trash/", import.meta.url));

/** Адрес клиента в хранилище e2e: `/v/vault<path>`. */
export const vaultUrl = (path = "/") => `/v/${VAULT_NAME}${path}`;

/** Адрес заметки, как его строит клиент. */
export const noteUrl = (id: string, anchor?: string) =>
  vaultUrl(`/n/${id.split("/").map(encodeURIComponent).join("/")}${anchor ? `#${encodeURIComponent(anchor)}` : ""}`);

/** Путь запроса API без хранилища: `/api/vaults/vault/notes/A` → `/api/notes/A`. */
export const apiPath = (url: URL) => url.pathname.replace(/^\/api\/vaults\/[^/]+\//, "/api/");

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

/**
 * Настройки общие для всех сценариев: вернуть тему «как в системе» — общую
 * и хранилища (тема из интерфейса меняется только для хранилища).
 */
export async function resetTheme(page: Page) {
  for (const [url, value] of [
    ["/api/settings", "auto"],
    [`/api/vaults/${VAULT_NAME}/settings`, null],
  ] as const) {
    const res = await page.request.put(url, { data: { "appearance.theme": value } });
    if (!res.ok()) throw new Error(`тема не сброшена: ${res.status()}`);
  }
}
