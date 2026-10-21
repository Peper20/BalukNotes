// The core in a Web Worker (docs/architecture.md, 1): with `__NOTES_API__.core`
// the client sends its `/api/...` requests to the notes-wasm module in a
// worker, so a note opens and builds in the browser itself. The module and the
// vault files (tests/.data/wasm, made by the `wasm-module` step of check.sh)
// are answered under /wasm/ by the test; the server on 8433 serves the client
// and the stylesheets only. The first build in WASM takes tens of seconds.
// The worker keeps the vault and the settings in OPFS (`worker/opfs.ts`).
import { existsSync, readFileSync } from "node:fs";
import { extname } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { noteUrl, VAULT_NAME } from "./helpers";

const WASM_DIR = fileURLToPath(new URL("../../tests/.data/wasm/", import.meta.url));
const TYPES: Record<string, string> = { ".js": "text/javascript", ".wasm": "application/wasm", ".json": "application/json" };

/** Clears OPFS of the origin: the page is a plain file of the server, so no core runs while it is cleared. */
async function clearOpfs(page: Page): Promise<void> {
  await page.goto("/assets/icon.svg");
  await page.evaluate(async () => {
    const root = await navigator.storage.getDirectory();
    await root.removeEntry("notes", { recursive: true }).catch((e: DOMException) => {
      if (e.name !== "NotFoundError") throw e;
    });
  });
}

/** The text of an OPFS file (path from the root), `null` if there is none. */
const opfsText = (page: Page, path: string) =>
  page.evaluate(async (path) => {
    try {
      const names = path.split("/");
      const file = names.pop() as string;
      let dir = await navigator.storage.getDirectory();
      for (const name of names) dir = await dir.getDirectoryHandle(name);
      return await (await (await dir.getFileHandle(file)).getFile()).text();
    } catch {
      return null;
    }
  }, path);

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

/** What the server of the page still answers when the core runs in the worker. */
const SERVER_PATHS = new Set(["/api/fonts.css", "/api/themes.css", "/api/session"]);

const settled = (page: Page) => expect(page.locator("html")).toHaveAttribute("data-state", "ready", { timeout: 180_000 });

test("a note builds in the browser: formula, CeTZ figure, the list and search; the server gets no vault request", async ({ page }) => {
  test.setTimeout(300_000);
  await clearOpfs(page);
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

  // Only the stylesheets and the session (sign-in is the server's) come from the server; not a single vault, note or settings request.
  expect(served.filter((path) => !SERVER_PATHS.has(path))).toEqual([]);
  expect(errors).toEqual([]);
});

test("the vault and the settings stay in OPFS: a reload reads them back, files.json is fetched once", async ({ page }) => {
  test.setTimeout(400_000);
  await clearOpfs(page);
  const served = await withCore(page);
  let seeds = 0;
  page.on("request", (request) => {
    if (new URL(request.url()).pathname === "/wasm/files.json") seeds++;
  });
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));

  await page.goto(noteUrl("Сеть/SSH"));
  await settled(page);
  await expect(page.locator("#note .k-title h1")).toHaveText("SSH");
  expect(seeds).toBe(1);

  // A shared setting (the font size) and a vault setting (the tags are hidden in this vault only).
  await page.locator("#open-settings").click();
  const size = page.locator('input[id="setting-appearance.font_size"]');
  await size.fill("23");
  await size.dispatchEvent("change");
  await expect(page.locator("html")).toHaveCSS("--k-size", "23px");
  const tags = page.locator('.setting[data-key="header.tags"]');
  await tags.locator("input").uncheck();
  await expect(tags.locator(".setting-badge")).toHaveText("только в этом хранилище");

  // The worker writes them through after the answer; wait for the files before the reload.
  await expect.poll(() => opfsText(page, "notes/settings.json")).toContain('"appearance.font_size":23');
  await expect.poll(() => opfsText(page, "notes/vaults/vault/files/.baluk/settings.json")).toContain('"header.tags": false');
  expect(await opfsText(page, "notes/vaults/vault/files/Сеть/SSH.typ")).toContain("SSH");

  await page.reload();
  await settled(page);
  await expect(page.locator("#note .k-title h1")).toHaveText("SSH");
  await expect(page.locator("html")).toHaveCSS("--k-size", "23px");
  await page.locator("#open-settings").click();
  await expect(page.locator('.setting[data-key="appearance.font_size"] .setting-badge')).toHaveText("изменено для всех хранилищ");
  await expect(page.locator('.setting[data-key="header.tags"] .setting-badge')).toHaveText("только в этом хранилище");

  expect(seeds, "the reload read OPFS, not files.json").toBe(1);
  expect(served.filter((path) => !SERVER_PATHS.has(path))).toEqual([]);
  expect(errors).toEqual([]);
});
