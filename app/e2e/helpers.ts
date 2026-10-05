import { fileURLToPath } from "node:url";
import { expect, type Page } from "@playwright/test";

/** The vault of the e2e server: a copy of tests/vault (see playwright.config.ts). */
export const VAULT_NAME = "vault";
export const VAULT = fileURLToPath(new URL(`../../tests/.data/e2e/vaults/${VAULT_NAME}/`, import.meta.url));
/** The trash of the e2e server: deleted notes go here, not to the system trash. */
export const TRASH = fileURLToPath(new URL("../../tests/.data/e2e/trash/", import.meta.url));

/** A client address in the e2e vault: `/v/vault<path>`. */
export const vaultUrl = (path = "/") => `/v/${VAULT_NAME}${path}`;

/** A note address, as the client builds it. */
export const noteUrl = (id: string, anchor?: string) =>
  vaultUrl(`/n/${id.split("/").map(encodeURIComponent).join("/")}${anchor ? `#${encodeURIComponent(anchor)}` : ""}`);

/** An API request path without the vault: `/api/vaults/vault/notes/A` -> `/api/notes/A`. */
export const apiPath = (url: URL) => url.pathname.replace(/^\/api\/vaults\/[^/]+\//, "/api/");

/** The client has drawn a note or home (<html data-state="ready">). */
export async function ready(page: Page) {
  await expect(page.locator("html")).toHaveAttribute("data-state", "ready");
}

export async function open(page: Page, id: string, anchor?: string) {
  await page.goto(noteUrl(id, anchor));
  await ready(page);
}

/** The note heading (the title h1). */
export const title = (page: Page) => page.locator("#note .k-title h1");

/**
 * Settings shared by all scenarios: brings the theme back to "auto", the
 * shared one and the vault's (a theme from the interface changes only for the vault).
 */
export async function resetTheme(page: Page) {
  for (const [url, value] of [
    ["/api/settings", "auto"],
    [`/api/vaults/${VAULT_NAME}/settings`, null],
  ] as const) {
    const res = await page.request.put(url, { data: { "appearance.theme": value } });
    if (!res.ok()) throw new Error(`theme not reset: ${res.status()}`);
  }
}
