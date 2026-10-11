// Sign-in on the TCP address (`notes serve --auth`): this spec starts its OWN
// server (port 8441, data tests/.data/auth-e2e: a copy of the test vault and
// one account made with the `notes-hub` binary), the shared e2e server runs
// without sign-in. The address shows the sign-in screen, a wrong password
// says so, the right one opens the vault, a reload stays signed in, sign-out
// returns to the screen and the API answers 401 again.
import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { cpSync, mkdirSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { VAULT_NAME } from "./helpers";

const PORT = 8441;
const BASE = `http://127.0.0.1:${PORT}`;
const ROOT = fileURLToPath(new URL("../../", import.meta.url));
const DATA = join(ROOT, "tests/.data/auth-e2e");
const BIN = join(process.env.CARGO_TARGET_DIR ?? join(ROOT, "target"), "debug");
const LOGIN = "test";
const PASSWORD = "correct-horse-9";

let server: ChildProcess | undefined;

async function waitFor(url: string, ms: number) {
  const until = Date.now() + ms;
  for (;;) {
    try {
      if ((await fetch(url)).ok) return;
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
  mkdirSync(join(DATA, "vaults"), { recursive: true });
  cpSync(join(ROOT, "tests/vault"), join(DATA, "vaults", VAULT_NAME), { recursive: true });
  execFileSync(join(BIN, "notes-hub"), ["--data", DATA, "users", "add", LOGIN, "--password-stdin"], { input: `${PASSWORD}\n`, stdio: ["pipe", "ignore", "inherit"] });
  server = spawn(join(BIN, "notes-typst"), ["--data", DATA, "--trash", join(DATA, "trash"), "serve", "--addr", `127.0.0.1:${PORT}`, "--auth"], {
    stdio: ["ignore", "ignore", "inherit"],
  });
  // The sign-in screen's own files are open.
  await waitFor(`${BASE}/api/themes`, 120_000);
  // Build the notes in advance (the Bearer header is how a program signs in).
  const res = await fetch(`${BASE}/api/login`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ login: LOGIN, password: PASSWORD }) });
  const { token } = (await res.json()) as { token: string };
  const headers = { Authorization: `Bearer ${token}` };
  const api = `${BASE}/api/vaults/${VAULT_NAME}`;
  const list = (await (await fetch(`${api}/notes`, { headers })).json()) as { id: string }[];
  for (const { id } of list) await (await fetch(`${api}/notes/${id.split("/").map(encodeURIComponent).join("/")}`, { headers })).arrayBuffer();
  await fetch(`${BASE}/api/logout`, { method: "POST", headers });
});

test.afterAll(async () => {
  server?.kill("SIGTERM");
  await new Promise((r) => setTimeout(r, 300));
});

test("sign-in: the screen, a wrong password, the right one, a reload, sign-out", async ({ page, context }) => {
  const consoleErrors: string[] = [];
  page.on("pageerror", (e) => consoleErrors.push(e.message));

  // An address of the app without a session: the sign-in screen, not the app.
  await page.goto(`${BASE}/v/${VAULT_NAME}/`);
  await expect(page.locator("html")).toHaveAttribute("data-state", "ready");
  const screen = page.locator("#sign-in");
  await expect(screen).toBeVisible();
  await expect(page.locator("#vault-name")).toHaveCount(0);
  expect((await page.request.get(`${BASE}/api/vaults`)).status()).toBe(401);
  expect((await page.request.get(`${BASE}/api/session`)).status()).toBe(401);

  // The fields are for password managers.
  await expect(screen.getByLabel("Логин")).toHaveAttribute("autocomplete", "username");
  await expect(screen.getByLabel("Пароль")).toHaveAttribute("autocomplete", "current-password");
  await expect(screen.getByRole("button", { name: "Войти" })).toBeDisabled();

  // A wrong login or password: the error under the form, still the screen.
  await screen.getByLabel("Логин").fill("nobody");
  await screen.getByLabel("Пароль").fill("wrong-password");
  await screen.getByLabel("Пароль").press("Enter");
  await expect(screen.getByRole("alert")).toHaveText("Неверный логин или пароль");
  await expect(screen).toBeVisible();

  // The right one (Enter submits): the page reloads to the same address, the app is drawn.
  await screen.getByLabel("Логин").fill(LOGIN);
  await screen.getByLabel("Пароль").fill(PASSWORD);
  await screen.getByRole("button", { name: "Войти" }).click();
  await expect(page.locator("#vault-name")).toHaveText(VAULT_NAME, { timeout: 30_000 });
  await expect(page).toHaveURL(`${BASE}/v/${VAULT_NAME}/`);
  expect(page.url()).not.toContain("token");
  const [cookie] = (await context.cookies(BASE)).filter((c) => c.name === "notes_session");
  expect(cookie).toMatchObject({ httpOnly: true, sameSite: "Strict" });
  expect((await page.request.get(`${BASE}/api/vaults`)).status()).toBe(200);

  // A reload stays signed in.
  await page.reload();
  await expect(page.locator("#vault-name")).toHaveText(VAULT_NAME, { timeout: 30_000 });
  await expect(page.locator("html")).toHaveAttribute("data-state", "ready");

  // Sign-out is at the bottom of the vault menu and returns to the sign-in screen.
  await page.locator("#vault-switch").click();
  await expect(page.locator("#sign-out")).toHaveText(`Выйти (${LOGIN})`);
  await page.locator("#sign-out").click();
  await expect(page.locator("#sign-in")).toBeVisible();
  expect((await page.request.get(`${BASE}/api/vaults`)).status()).toBe(401);
  await page.reload();
  await expect(page.locator("#sign-in")).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

test("a session that ended while the app is open: the sign-in screen, no 'нет связи'", async ({ page }) => {
  await page.goto(`${BASE}/v/${VAULT_NAME}/`);
  await page.locator("#sign-in").getByLabel("Логин").fill(LOGIN);
  await page.locator("#sign-in").getByLabel("Пароль").fill(PASSWORD);
  await page.locator("#sign-in").getByRole("button", { name: "Войти" }).click();
  await expect(page.locator("#vault-name")).toHaveText(VAULT_NAME, { timeout: 30_000 });
  // The session ends elsewhere (another tab, the server): the next request is refused.
  await page.request.post(`${BASE}/api/logout`);
  // Opening the vault menu asks the server; the waiting events request may get there first - then the app is gone already.
  await page.locator("#vault-switch").click({ timeout: 3000 }).catch(() => {});
  await expect(page.locator("#sign-in")).toBeVisible();
  await expect(page.getByText("нет связи")).toHaveCount(0);
});
