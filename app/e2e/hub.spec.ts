// The browser version served by the hub (docs/architecture.md, 1 and 9):
// `notes-hub --web tests/.data/web` on port 8463 over tests/.data/e2e-hub, an
// account `e2e` and a hub vault `Учёба` (both made by playwright.config.ts
// before the server starts). The page asks the hub to sign in and for the
// vault list, and runs the core in the worker for the vault of the tab: the
// hub gets no vault, note or settings request. The vault's files do not reach
// the worker yet (sync is the next step), so an opened vault is empty.
// The first start of the module in the browser takes tens of seconds.
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
