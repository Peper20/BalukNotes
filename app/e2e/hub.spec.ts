// The browser version served by the hub (docs/architecture.md, 1 and 9):
// `notes-hub --web tests/.data/web` on HUB_PORT (e2e/ports.ts) over tests/.data/e2e-hub, an
// account `e2e` and a hub vault `Учёба` (both made by playwright.config.ts
// before the server starts). The page asks the hub to sign in and for the
// vault list, and runs the core in the worker for the vault of the tab: the
// hub gets no vault, note or settings request, only the sync API, with which
// the worker syncs the vault (a round at start, every 30 s, 2 s after a local
// change; the sync scenario sets the interval to 1 s, see `shortRounds`). The
// first start of the module in the browser takes tens of seconds.
import { expect, test } from "@playwright/test";
import { HUB_LOGIN, HUB_PASSWORD, HUB_URL, HUB_VAULT } from "./hub-env";

const NEW_VAULT = "Новое";
const ready = (page: import("@playwright/test").Page) => expect(page.locator("html")).toHaveAttribute("data-state", "ready", { timeout: 180_000 });

/** The paths the hub got under /api/ from the page (the requests of `page.request` are not among them). */
const hubApi = (page: import("@playwright/test").Page): string[] => {
  const served: string[] = [];
  page.on("request", (request) => {
    const { pathname } = new URL(request.url());
    if (pathname.startsWith("/api/")) served.push(pathname);
  });
  return served;
};

/**
 * The rounds of the worker every second instead of 30: the page gets the
 * bundle's `assets/config.js` with `core.syncEvery` added (`CoreSource.syncEvery`;
 * the bundle itself does not set it).
 */
async function shortRounds(page: import("@playwright/test").Page) {
  await page.route("**/assets/config.js", async (route) => {
    const config = await route.fetch();
    await route.fulfill({ response: config, body: `${await config.text()}\nglobalThis.__NOTES_API__.core.syncEvery = 1000;\n` });
  });
}

test("the hub's browser version: sign-in, the vault list from the hub, a new vault, the core in the worker", async ({ page }) => {
  test.setTimeout(400_000);
  const served = hubApi(page);
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));

  // No session: the sign-in screen.
  await page.goto(`${HUB_URL}/`);
  await expect(page.locator("#sign-in")).toBeVisible();
  await page.getByLabel("Логин").fill(HUB_LOGIN);
  await page.getByLabel("Пароль").fill(HUB_PASSWORD);
  await page.getByRole("button", { name: "Войти" }).click();

  // The picker lists the hub's vault.
  const picker = page.locator("#vault-picker");
  await expect(picker).toBeVisible();
  await ready(page);
  await expect(picker.getByRole("link", { name: HUB_VAULT })).toBeVisible();

  // A new vault from the picker: the hub makes it, the client opens it.
  await picker.getByLabel("Новое хранилище").fill(NEW_VAULT);
  await picker.getByRole("button", { name: "Создать" }).click();
  await expect(page).toHaveURL(`${HUB_URL}/v/${encodeURIComponent(NEW_VAULT)}/`);
  await ready(page);
  await expect(page.locator("#vault-name")).toHaveText(NEW_VAULT);
  const onHub = (await (await page.request.get(`${HUB_URL}/api/sync/vaults`)).json()) as { name: string }[];
  expect(onHub.map((v) => v.name).sort()).toEqual([NEW_VAULT, HUB_VAULT].sort());

  // The vault is the core's, in the worker, and empty: no notes in the list.
  await expect(page.locator("#tree a")).toHaveCount(0);

  // The vault menu is the hub's list too; the vault cannot be renamed here.
  await page.locator("#vault-switch").click();
  const menu = page.locator("#vault-menu");
  await expect(menu.getByRole("menuitem", { name: HUB_VAULT, exact: true })).toBeVisible();
  await expect(menu.getByRole("menuitem", { name: NEW_VAULT, exact: true })).toBeVisible();
  await page.locator("#vault-rename").click();
  const dialog = page.locator("#vault-edit");
  await dialog.getByLabel("Название").fill("Другое");
  await dialog.getByRole("button", { name: "Переименовать" }).click();
  await expect(dialog.getByRole("alert")).toHaveText("В браузере хранилище нельзя переименовать или удалить");
  expect(((await (await page.request.get(`${HUB_URL}/api/sync/vaults`)).json()) as { name: string }[]).map((v) => v.name)).toContain(NEW_VAULT);

  // The hub got the sign-in, the session, the sync API and the stylesheets, nothing of the vault.
  const allowed = new Set(["/api/login", "/api/session", "/api/fonts.css", "/api/themes.css"]);
  expect(served.filter((path) => !allowed.has(path) && !path.startsWith("/api/sync/"))).toEqual([]);
  expect(served).toContain("/api/sync/vaults");
  expect(errors).toEqual([]);
});

test("the worker syncs the vault with the hub: download, a change on the hub, OPFS after a reload, a deletion", async ({ page }) => {
  test.setTimeout(400_000);
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await shortRounds(page);
  // The sync requests of the worker (the context sees those of workers too).
  const vaultApi = `/api/sync/vaults/${HUB_VAULT}`;
  const fileGets: string[] = [];
  const changeGets: string[] = [];
  page.context().on("request", (request) => {
    const path = decodeURIComponent(new URL(request.url()).pathname);
    if (request.method() === "GET" && path.startsWith(`${vaultApi}/files/`)) fileGets.push(path);
    if (request.method() === "GET" && path === `${vaultApi}/changes`) changeGets.push(path);
  });
  const fileUrl = `${HUB_URL}/api/sync/vaults/${encodeURIComponent(HUB_VAULT)}/files/${encodeURIComponent("Заметка.typ")}`;

  await page.goto(`${HUB_URL}/`);
  await page.getByLabel("Логин").fill(HUB_LOGIN);
  await page.getByLabel("Пароль").fill(HUB_PASSWORD);
  await page.getByRole("button", { name: "Войти" }).click();
  await expect(page.locator("#vault-picker")).toBeVisible();

  // The vault of the hub opens with its note, downloaded by the first round.
  await page.goto(`${HUB_URL}/v/${encodeURIComponent(HUB_VAULT)}/`);
  await ready(page);
  // By its address: the tree shows the file name until the note is built, the title after.
  const link = page.locator(`#tree a[href$="/n/${encodeURIComponent("Заметка")}"]`);
  await link.click();
  const note = page.locator("#note");
  await expect(note).toContainText("Заметка хаба", { timeout: 180_000 });
  expect(fileGets).toEqual([`${vaultApi}/files/Заметка.typ`]);

  // Another device changes the note on the hub: the open note shows it by itself.
  const current = await page.request.get(fileUrl);
  expect(current.status()).toBe(200);
  const put = await page.request.put(fileUrl, {
    data: Buffer.from("= Заметка хаба\n\nНовый текст с хаба.\n"),
    headers: { "x-base": current.headers()["x-hash"]!, "content-type": "application/octet-stream" },
  });
  expect(put.status()).toBe(200);
  await expect(note).toContainText("Новый текст с хаба", { timeout: 15_000 });

  // A reload opens the note from OPFS: the start round asks for changes, no file is downloaded.
  fileGets.length = 0;
  changeGets.length = 0;
  await page.reload();
  await ready(page);
  await expect(note).toContainText("Новый текст с хаба", { timeout: 180_000 });
  expect(changeGets.length).toBeGreaterThan(0);
  expect(fileGets).toEqual([]);

  // A note deleted in the browser is deleted on the hub.
  await link.click({ button: "right" });
  await page.locator("#note-menu").getByRole("menuitem", { name: "Удалить…" }).click();
  await page.locator("#note-delete-confirm").click();
  await expect(link).toHaveCount(0);
  const onHub = async () => {
    const res = await page.request.get(`${HUB_URL}/api/sync/vaults/${encodeURIComponent(HUB_VAULT)}/changes?after=0`);
    const changes = (await res.json()) as { entries: { path: string; hash: string | null }[] };
    return changes.entries.find((e) => e.path === "Заметка.typ")?.hash;
  };
  await expect.poll(onHub, { timeout: 10_000 }).toBeNull();
  expect(errors).toEqual([]);
});
