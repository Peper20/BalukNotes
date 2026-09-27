#!/usr/bin/env node
// Скрипт статического сайта (`notes build`): src/static/* → dist/assets/
// static*.js. Каждая часть — один файл без модулей (IIFE): сайт открывается
// и с диска (file://). `static.js` (тема, оглавление) — у каждой страницы,
// остальные части он грузит по требованию (`src/static/parts.ts`).
// Собирается после основного клиента (`npm run build`), в тот же dist.
//
// Новая часть — вход в src/static/ + строка в PARTS + поле в `Parts`
// (`parts.ts`). Сборка сайта (`notes-site`) копирует все `static*.js`.
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { build } from "vite";

/** Файл в assets/ → вход. */
const PARTS = {
  "static.js": "src/static/main.ts",
  "static-live.js": "src/static/live.ts",
  "static-search.js": "src/static/search.ts",
  "static-graph.js": "src/static/graph.ts",
};

for (const [file, entry] of Object.entries(PARTS)) {
  await build({
    configFile: false,
    root: new URL("..", import.meta.url).pathname,
    logLevel: "warn",
    plugins: [svelte()],
    publicDir: false,
    build: {
      outDir: "dist",
      emptyOutDir: false,
      target: "es2023",
      lib: { entry, formats: ["iife"], name: "balukStatic", fileName: () => `assets/${file}` },
    },
  });
}
