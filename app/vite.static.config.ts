// Скрипт статического сайта (`notes build`): src/static.ts → dist/assets/static.js,
// один файл без модулей (IIFE) — сайт открывается и с диска (file://).
// Собирается после основного клиента (`npm run build`), в тот же dist.
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [svelte()],
  publicDir: false,
  build: {
    outDir: "dist",
    emptyOutDir: false,
    target: "es2023",
    lib: { entry: "src/static.ts", formats: ["iife"], name: "balukStatic", fileName: () => "assets/static.js" },
  },
});
