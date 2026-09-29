// Сквозные тесты интерфейса: настоящий сервер notes на каталоге данных
// tests/.data/e2e с копией тестового хранилища (vaults/vault — сценарии могут
// создавать заметки и хранилища, не трогая фикстуры; удалённое — в
// tests/.data/e2e/trash, не в корзину системы), портом 8433; клиент — сборка app/dist.
// Браузер — системный chromium, Playwright ничего не скачивает. До сценариев
// сервер прогревается (e2e/global-setup.ts): все заметки уже собраны, поэтому
// ожидания короткие; первую сборку проверяет 01-switch (сам меняет файлы).
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
      `cargo run -q -p notes-cli -- --data tests/.data/e2e --trash tests/.data/e2e/trash serve --addr 127.0.0.1:${port}`,
    ].join(" && "),
    cwd: "..",
    url: `http://127.0.0.1:${port}/api/vaults`,
    timeout: 300_000,
    reuseExistingServer: false,
  },
});
