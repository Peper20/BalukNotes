// End-to-end interface tests. The client is the app/dist build, the browser is
// the system chromium, Playwright downloads nothing. All ports of the e2e lie in
// 8600-8649 (e2e/ports.ts).
//
// App servers: every Playwright worker has its own `notes-typst serve` (port
// 8600 + the worker's index) on its own copy of the test vault in
// tests/.data/e2e/w<index> (scenarios create and delete notes and vaults
// without touching the fixtures; deleted ones go to the trash inside that
// directory, not to the system trash): e2e/fixtures.ts. The files of scenarios
// run in parallel, the tests of one file in order. Before the scenarios
// e2e/global-setup.ts builds the binaries and warms a template of the data
// (all notes built), which the workers copy; the first build is checked by
// 01-switch (it changes the files itself).
//
// A server that is not per worker: `notes-hub` on HUB_PORT over
// tests/.data/e2e-hub with the browser version (tests/.data/web, made by the
// `web-bundle` step of tools/check.sh), for e2e/hub.spec.ts: the account `e2e`,
// the hub vault `Учёба` with one note, put into the data directory before the
// server starts. Specs with servers of their own (auth, sync) start them
// themselves.
//
// The number of workers is PLAYWRIGHT_WORKERS (tools/lib/cpu-cap.sh sets it in
// tools/check.sh), at most MAX_WORKERS; unset: one worker.
//
//   npm run build && npm run e2e
import { defineConfig } from "@playwright/test";
import { HUB_DATA, HUB_LOGIN, HUB_PASSWORD, HUB_URL, HUB_VAULT } from "./e2e/hub-env";
import { HUB_PORT, MAX_WORKERS } from "./e2e/ports";
import { BIN, BUILD } from "./e2e/server";

const requested = Number(process.env.PLAYWRIGHT_WORKERS ?? 1);
if (!Number.isInteger(requested) || requested < 1) throw new Error(`PLAYWRIGHT_WORKERS must be a positive integer, got: ${process.env.PLAYWRIGHT_WORKERS}`);
const workers = Math.min(requested, MAX_WORKERS);

export default defineConfig({
  testDir: "e2e",
  timeout: 60_000,
  expect: { timeout: 10_000 },
  globalSetup: "./e2e/global-setup.ts",
  fullyParallel: false,
  workers,
  reporter: [["list"]],
  use: {
    // Never a window on the user's display (`.claude/rules/tools.md`).
    headless: true,
    launchOptions: { executablePath: process.env.CHROMIUM ?? "/usr/bin/chromium" },
    viewport: { width: 1300, height: 900 },
  },
  webServer: {
    // The hub finds the files put into <data>/hub/<login>/<vault>/files/ when it opens the vault (HubVault::open rescans).
    command: [
      `cargo ${BUILD.join(" ")}`,
      `rm -rf ${HUB_DATA}`,
      `mkdir -p ${HUB_DATA}/hub/${HUB_LOGIN}/${HUB_VAULT}/files`,
      `printf '= Заметка хаба\\n' > ${HUB_DATA}/hub/${HUB_LOGIN}/${HUB_VAULT}/files/Заметка.typ`,
      `echo ${HUB_PASSWORD} | ${BIN}/notes-hub --data ${HUB_DATA} users add ${HUB_LOGIN} --password-stdin`,
      `exec ${BIN}/notes-hub --data ${HUB_DATA} hub serve --addr 127.0.0.1:${HUB_PORT} --web tests/.data/web`,
    ].join(" && "),
    cwd: "..",
    url: `${HUB_URL}/`,
    timeout: 300_000,
    reuseExistingServer: false,
  },
});
