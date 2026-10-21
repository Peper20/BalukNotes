// End-to-end interface tests: a real notes server on the data directory
// tests/.data/e2e with a copy of the test vault (vaults/vault - scenarios may
// create notes and vaults without touching the fixtures; deleted ones go to
// tests/.data/e2e/trash, not to the system trash), port 8433; the client is
// the app/dist build. The browser is the system chromium, Playwright
// downloads nothing. Before the scenarios the server is warmed
// (e2e/global-setup.ts): all notes are built, so the waits are short; the
// first build is checked by 01-switch (it changes the files itself).
//
// A second server, `notes-hub` on port 8463 over tests/.data/e2e-hub with the
// browser version (tests/.data/web, made by the `web-bundle` step of
// tools/check.sh), serves e2e/hub.spec.ts: the account `e2e`, the hub vault
// `Учёба` with one note, put into the data directory before the server starts.
//
//   npm run build && npm run e2e
import { defineConfig } from "@playwright/test";
import { HUB_DATA, HUB_LOGIN, HUB_PASSWORD, HUB_PORT, HUB_URL, HUB_VAULT } from "./e2e/hub-env";

const port = 8433;

export default defineConfig({
  testDir: "e2e",
  timeout: 60_000,
  expect: { timeout: 10_000 },
  globalSetup: "./e2e/global-setup.ts",
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    // Never a window on the user's display (`.claude/rules/tools.md`).
    headless: true,
    launchOptions: { executablePath: process.env.CHROMIUM ?? "/usr/bin/chromium" },
    viewport: { width: 1300, height: 900 },
  },
  webServer: [
    {
      command: [
        "rm -rf tests/.data/e2e",
        "mkdir -p tests/.data/e2e/vaults",
        "cp -r tests/vault tests/.data/e2e/vaults/vault",
        `cargo run -q -p notes-typst -- --data tests/.data/e2e --trash tests/.data/e2e/trash serve --addr 127.0.0.1:${port}`,
      ].join(" && "),
      cwd: "..",
      url: `http://127.0.0.1:${port}/api/vaults`,
      timeout: 300_000,
      reuseExistingServer: false,
    },
    {
      // The hub finds the files put into <data>/hub/<login>/<vault>/files/ when it opens the vault (HubVault::open rescans).
      command: [
        `rm -rf ${HUB_DATA}`,
        `mkdir -p ${HUB_DATA}/hub/${HUB_LOGIN}/${HUB_VAULT}/files`,
        `printf '= Заметка хаба\\n' > ${HUB_DATA}/hub/${HUB_LOGIN}/${HUB_VAULT}/files/Заметка.typ`,
        `echo ${HUB_PASSWORD} | cargo run -q -p notes-hub -- --data ${HUB_DATA} users add ${HUB_LOGIN} --password-stdin`,
        `cargo run -q -p notes-hub -- --data ${HUB_DATA} hub serve --addr 127.0.0.1:${HUB_PORT} --web tests/.data/web`,
      ].join(" && "),
      cwd: "..",
      url: `${HUB_URL}/`,
      timeout: 300_000,
      reuseExistingServer: false,
    },
  ],
});
