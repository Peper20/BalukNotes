// End-to-end interface tests: a real notes server on the data directory
// tests/.data/e2e with a copy of the test vault (vaults/vault - scenarios may
// create notes and vaults without touching the fixtures; deleted ones go to
// tests/.data/e2e/trash, not to the system trash), port 8433; the client is
// the app/dist build. The browser is the system chromium, Playwright
// downloads nothing. Before the scenarios the server is warmed
// (e2e/global-setup.ts): all notes are built, so the waits are short; the
// first build is checked by 01-switch (it changes the files itself).
//
//   npm run build && npm run e2e
import { defineConfig } from "@playwright/test";

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
    launchOptions: { executablePath: process.env.CHROMIUM ?? "/usr/bin/chromium" },
    viewport: { width: 1300, height: 900 },
  },
  webServer: {
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
});
