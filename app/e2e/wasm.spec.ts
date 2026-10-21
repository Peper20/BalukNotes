// The core in a Web Worker (docs/architecture.md, 1): with `__NOTES_API__.core`
// the client sends its `/api/...` requests to the notes-wasm module in a
// worker, so a note opens and builds in the browser itself. The module and the
// vault files (tests/.data/wasm, made by the `wasm-module` step of check.sh)
// are answered under /wasm/ by the test; the server on 8433 serves the client
// and the stylesheets only. The first build in WASM takes tens of seconds.
import { existsSync, readFileSync } from "node:fs";
import { extname } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { noteUrl, VAULT_NAME } from "./helpers";

const WASM_DIR = fileURLToPath(new URL("../../tests/.data/wasm/", import.meta.url));
const TYPES: Record<string, string> = { ".js": "text/javascript", ".wasm": "application/wasm", ".json": "application/json" };

/** The page of the e2e server with the core in a worker; returns the paths of the requests the server got under /api/. */
async function withCore(page: Page): Promise<string[]> {
  await page.addInitScript((vault) => {
    globalThis.__NOTES_API__ = { core: { module: "/wasm/pkg/notes_wasm.js", vault, files: "/wasm/files.json" } };
  }, VAULT_NAME);
  // The route must see the requests of the worker too (module, .wasm, files.json).
  await page.route("**/wasm/**", async (route) => {
    const file = new URL(route.request().url()).pathname.replace(/^\/wasm\//, "");
    if (!existsSync(WASM_DIR + file)) return route.fulfill({ status: 404 });
    await route.fulfill({ body: readFileSync(WASM_DIR + file), contentType: TYPES[extname(file)] ?? "application/octet-stream" });
  });
  const served: string[] = [];
  page.on("request", (request) => {
    const { pathname } = new URL(request.url());
    if (pathname.startsWith("/api/")) served.push(pathname);
  });
  return served;
}

const settled = (page: Page) => expect(page.locator("html")).toHaveAttribute("data-state", "ready", { timeout: 180_000 });

test("a note builds in the browser: formula, CeTZ figure, the list and search; the server gets no vault request", async ({ page }) => {
  test.setTimeout(300_000);
  const served = await withCore(page);
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));

  await page.goto(noteUrl("Особые случаи/Свои слова"));
  await settled(page);
  const note = page.locator("#note");
  await expect(note.locator(".k-title h1")).toHaveText("Eigene Wörter");
  await expect(note.locator("math").first()).toBeVisible();
  await expect(note.locator("figure svg").first()).toBeVisible();

  // The note list (the tree) comes from the core.
  await expect(page.locator("#tree a", { hasText: "SSH" })).toHaveCount(1);

  // Search answers from the core.
  await page.keyboard.press("Control+o");
  await page.locator(".palette input").fill("/итоги второй");
  await expect(page.locator(".palette-list li[role=option]").first()).toContainText("Тестовая книга › Итоги", { timeout: 60_000 });

  // Another note, built in the worker as well.
  await page.locator(".palette input").fill("ssh");
  await page.keyboard.press("Enter");
  await settled(page);
  await expect(page.locator("#note .k-title h1")).toHaveText("SSH");

  // Only the stylesheets come from the server; not a single vault, note or settings request.
  expect(served.filter((path) => path !== "/api/fonts.css" && path !== "/api/themes.css")).toEqual([]);
  expect(errors).toEqual([]);
});
