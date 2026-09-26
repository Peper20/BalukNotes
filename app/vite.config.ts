// Клиент baluk notes. Сборка — app/dist, её встраивает notes-server:
// index.html — на любой адрес клиента (/, /n/…), остальное — /assets/….
//
// Разработка: `npm run dev` — Vite с горячей заменой, API и шрифты — прокси
// на сервер notes (по умолчанию тестовое окружение tools/test-env.sh, :8432;
// другой — NOTES_API=http://127.0.0.1:8421 npm run dev).
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

const api = process.env.NOTES_API ?? "http://127.0.0.1:8432";

export default defineConfig({
  plugins: [svelte()],
  build: { outDir: "dist", emptyOutDir: true, assetsDir: "assets", target: "es2023" },
  server: {
    port: 5173,
    strictPort: true,
    proxy: { "/api": api, "/fonts": api },
    // Снимки отрисовки — эталон для тестов разборщика формул (lib/plot).
    fs: { allow: [".", "../tests/snapshots"] },
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "happy-dom",
  },
});
