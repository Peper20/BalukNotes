// Сквозные тесты интерфейса: настоящий сервер notes на копии тестового
// хранилища (tests/.data/e2e/vault — сценарии могут создавать заметки, не
// трогая фикстуры), своими данными и портом 8433; клиент — сборка app/dist.
// Браузер — системный chromium, Playwright ничего не скачивает.
//
//   npm run build && npm run e2e
import { defineConfig } from "@playwright/test";

const port = 8433;

export default defineConfig({
  testDir: "e2e",
  timeout: 60_000,
  expect: { timeout: 30_000 },
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
      "mkdir -p tests/.data/e2e",
      "cp -r tests/vault tests/.data/e2e/vault",
      `cargo run -q -p notes-cli -- --data tests/.data/e2e serve --addr 127.0.0.1:${port}`,
    ].join(" && "),
    cwd: "..",
    url: `http://127.0.0.1:${port}/api/notes`,
    timeout: 300_000,
    reuseExistingServer: false,
  },
});
