// Vault sync in the settings: this spec starts its OWN storage server (port
// 8461, `notes-hub`, one account) and its OWN app server (port 8462, a data
// directory with one vault), and a second device is the `notes` CLI on another
// data directory. Through the settings window: a wrong password and an
// unreachable server say so, the sign-in, the switch of a vault (the files
// reach the hub), a change made on the other device shows in the open note
// without a reload, a vault that is only on the server is downloaded and
// appears in the vault menu, the switch off, sign-out. The states the real
// servers cannot produce quickly (no sync on the server, an ended session) are
// answered by the page route.
import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { expect, test, type Page } from "@playwright/test";

const HUB_PORT = 8461;
const APP_PORT = 8462;
const HUB = `http://127.0.0.1:${HUB_PORT}`;
const APP = `http://127.0.0.1:${APP_PORT}`;
const ROOT = fileURLToPath(new URL("../../", import.meta.url));
const DATA = join(ROOT, "tests/.data/sync-e2e");
const DIR_HUB = join(DATA, "hub");
const DIR_A = join(DATA, "a");
const DIR_B = join(DATA, "b");
const BIN = join(process.env.CARGO_TARGET_DIR ?? join(ROOT, "target"), "debug");
const LOGIN = "test";
const PASSWORD = "correct-horse-9";
const VAULT = "notes";
const CLOUD = "Облако";

const note = (title: string, text: string) => `#import "/_baluk/lib.typ": *\n#show: note.with(title: [${title}])\n\n${text}\n`;
const write = (dir: string, vault: string, file: string, text: string) => {
  mkdirSync(join(dir, "vaults", vault), { recursive: true });
  writeFileSync(join(dir, "vaults", vault, file), text);
};
/** The `notes-typst` CLI on a device's data directory (the second device is one of these). */
const device = (dir: string, ...args: string[]) =>
  execFileSync(join(BIN, "notes-typst"), ["--data", dir, ...args], { input: `${PASSWORD}\n`, stdio: ["pipe", "pipe", "inherit"] }).toString();

let hub: ChildProcess | undefined;
let app: ChildProcess | undefined;

/** Waits until the address answers (any status when `any`). */
async function waitFor(url: string, ms: number, any = false) {
  const until = Date.now() + ms;
  for (;;) {
    try {
      if ((await fetch(url)).ok || any) return;
    } catch {
      // not listening yet
    }
    if (Date.now() > until) throw new Error(`no answer from ${url}`);
    await new Promise((r) => setTimeout(r, 200));
  }
}

test.describe.configure({ mode: "serial" });

test.beforeAll(async () => {
  test.setTimeout(300_000);
  execFileSync("cargo", ["build", "-q", "-p", "notes-typst", "-p", "notes-hub"], { cwd: ROOT, stdio: "inherit" });
  rmSync(DATA, { recursive: true, force: true });
  mkdirSync(DIR_HUB, { recursive: true });
  write(DIR_A, VAULT, "a.typ", note("Первая", "Текст до."));
  execFileSync(join(BIN, "notes-hub"), ["--data", DIR_HUB, "users", "add", LOGIN, "--password-stdin"], { input: `${PASSWORD}\n`, stdio: ["pipe", "ignore", "inherit"] });
  hub = spawn(join(BIN, "notes-hub"), ["--data", DIR_HUB, "hub", "serve", "--addr", `127.0.0.1:${HUB_PORT}`], { stdio: ["ignore", "ignore", "inherit"] });
  app = spawn(join(BIN, "notes-typst"), ["--data", DIR_A, "--trash", join(DATA, "trash"), "serve", "--addr", `127.0.0.1:${APP_PORT}`], {
    stdio: ["ignore", "ignore", "inherit"],
  });
  await waitFor(`${APP}/api/vaults`, 120_000);
  await waitFor(`${HUB}/api/session`, 30_000, true);
});

test.afterAll(async () => {
  hub?.kill("SIGTERM");
  app?.kill("SIGTERM");
  await new Promise((r) => setTimeout(r, 300));
});

/** The settings window opened on the sync section. */
async function openSync(page: Page) {
  await page.locator("#open-settings").click();
  const section = page.locator("#sync-settings");
  await expect(section).toBeVisible();
  return section;
}

test("sync: sign-in errors, sign-in, a vault goes to the server and a change from the other device comes back", async ({ page }) => {
  test.setTimeout(180_000);
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(`${APP}/v/${VAULT}/n/a`);
  await expect(page.locator("html")).toHaveAttribute("data-state", "ready");
  await expect(page.locator("#note")).toContainText("Текст до.");

  let section = await openSync(page);
  await expect(section.locator("legend")).toHaveText("Синхронизация");
  // The section is before "Это устройство".
  const legends = await page.locator("#settings legend").allTextContents();
  expect(legends.indexOf("Синхронизация")).toBe(legends.indexOf("Это устройство") - 1);
  const server = section.getByLabel("Сервер");
  await expect(server).toHaveAttribute("placeholder", "notes.example.org");
  const submit = section.getByRole("button", { name: "Войти" });
  await expect(submit).toBeDisabled();

  // A wrong password: the error under the form, nothing else changes; the dialog stays open on Enter.
  await server.fill(HUB);
  await section.getByLabel("Логин").fill(LOGIN);
  await section.getByLabel("Пароль").fill("wrong-password");
  await section.getByLabel("Пароль").press("Enter");
  await expect(section.getByRole("alert")).toHaveText("Неверный логин или пароль");
  await expect(page.locator("#settings")).toHaveJSProperty("open", true);
  expect((await (await page.request.get(`${APP}/api/device/sync`)).json()).signed_in).toBe(false);

  // A server that does not answer.
  await server.fill("http://127.0.0.1:8469");
  await section.getByLabel("Пароль").press("Enter");
  await expect(section.getByRole("alert")).toContainText("Сервер хранилища недоступен");

  // Plain http to another computer: a warning before the sign-in.
  await server.fill("http://notes.example.org");
  await expect(section.getByText("Соединение без шифрования")).toBeVisible();

  // The right one.
  await server.fill(HUB);
  await expect(section.getByText("Соединение без шифрования")).toHaveCount(0);
  // The hub slows the next attempt after a wrong password (a second).
  await page.waitForTimeout(1500);
  await section.getByLabel("Пароль").fill(PASSWORD);
  await submit.click();
  await expect(section.getByText(`${LOGIN} на ${HUB}`)).toBeVisible();
  await expect(section.getByLabel("Пароль")).toHaveCount(0);
  const row = section.locator(`[data-vault="${VAULT}"]`);
  await expect(row).toContainText("не синхронизируется");

  // The switch on: the first round runs, the file is on the hub.
  await row.getByRole("checkbox").check();
  await expect(row).toContainText("синхронизировано", { timeout: 60_000 });
  await expect(row.getByRole("checkbox")).toBeChecked();
  const onHub = join(DIR_HUB, "hub", LOGIN, VAULT, "files", "a.typ");
  await expect.poll(() => existsSync(onHub), { timeout: 20_000 }).toBe(true);
  expect(readFileSync(onHub, "utf8")).toContain("Текст до.");
  await row.getByRole("button", { name: "Синхронизировать сейчас" }).click();
  await expect(row).toContainText("синхронизировано");

  // The other device takes the vault and changes the note.
  mkdirSync(DIR_B, { recursive: true });
  device(DIR_B, "sync", "login", HUB, "--login", LOGIN, "--password-stdin");
  device(DIR_B, "--vault", VAULT, "sync", "link");
  const path = join(DIR_B, "vaults", VAULT, "a.typ");
  expect(readFileSync(path, "utf8")).toContain("Текст до.");
  writeFileSync(path, note("Первая", "Текст с другого устройства."));
  device(DIR_B, "--vault", VAULT, "sync", "now");

  // The note behind the dialog updates by itself (the worker pulls, the watcher refreshes), no reload.
  await page.keyboard.press("Escape");
  await expect(page.locator("#settings")).toHaveJSProperty("open", false);
  await expect(page.locator("#note")).toContainText("Текст с другого устройства.", { timeout: 40_000 });
  await expect(page.locator("#note")).not.toContainText("Текст до.");

  // A vault that exists only on the server: shown as such, the switch downloads it.
  write(DIR_B, CLOUD, "x.typ", note("Икс", "Из облака."));
  device(DIR_B, "--vault", CLOUD, "sync", "link");
  section = await openSync(page);
  const cloud = section.locator(`[data-vault="${CLOUD}"]`);
  await expect(cloud).toContainText("только на сервере");
  expect(existsSync(join(DIR_A, "vaults", CLOUD))).toBe(false);
  await cloud.getByRole("checkbox").check();
  await expect(cloud).toContainText("синхронизировано", { timeout: 60_000 });
  expect(readFileSync(join(DIR_A, "vaults", CLOUD, "x.typ"), "utf8")).toContain("Из облака.");
  // The vault is in the vault menu now.
  await page.keyboard.press("Escape");
  await page.locator("#vault-switch").click();
  await expect(page.locator("#vault-menu").getByRole("menuitem", { name: CLOUD, exact: true })).toBeVisible();
  await page.keyboard.press("Escape");

  // Switch off, sign out: the vault stays on both sides, the device is signed out.
  section = await openSync(page);
  await section.locator(`[data-vault="${CLOUD}"]`).getByRole("checkbox").uncheck();
  await expect(section.locator(`[data-vault="${CLOUD}"]`)).toContainText("не синхронизируется");
  await expect(section.locator(`[data-vault="${CLOUD}"]`).getByRole("button", { name: "Синхронизировать сейчас" })).toHaveCount(0);
  await section.locator(`[data-vault="${VAULT}"]`).getByRole("checkbox").uncheck();
  await expect(section.locator(`[data-vault="${VAULT}"]`)).toContainText("не синхронизируется");
  expect(existsSync(onHub)).toBe(true);
  await section.getByRole("button", { name: "Выйти" }).click();
  await expect(section.getByLabel("Пароль")).toBeVisible();
  expect((await (await page.request.get(`${APP}/api/device/sync`)).json()).signed_in).toBe(false);
  expect(errors).toEqual([]);
});

test("sync: a server without sync, an ended session", async ({ page }) => {
  await page.route("**/api/device/sync", (route) => route.fulfill({ status: 404, json: { error: "this server runs without vault sync" } }));
  await page.goto(`${APP}/v/${VAULT}/n/a`);
  await expect(page.locator("html")).toHaveAttribute("data-state", "ready");
  let section = await openSync(page);
  await expect(section).toContainText("Синхронизация недоступна на этом сервере");
  await expect(section.getByRole("button")).toHaveCount(0);
  await expect(section.getByRole("textbox")).toHaveCount(0);

  await page.unroute("**/api/device/sync");
  const status = {
    server: HUB,
    login: LOGIN,
    signed_in: true,
    server_error: "session ended",
    session_ended: true,
    vaults: [{ name: VAULT, local: true, remote: null, linked: true, state: "sign-in", last_sync: null, error: "the session ended", report: null }],
  };
  await page.route("**/api/device/sync", (route) => route.fulfill({ json: status }));
  await page.keyboard.press("Escape");
  section = await openSync(page);
  await expect(section.getByText("Сессия закончилась - войдите снова")).toBeVisible();
  await expect(section.getByLabel("Сервер")).toHaveValue(HUB);
  await expect(section.getByLabel("Логин")).toHaveValue(LOGIN);
  await expect(section.locator(`[data-vault="${VAULT}"]`)).toContainText("нужен вход");
});
