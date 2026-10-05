// The baluk notes client. The build is app/dist, embedded by notes-server:
// index.html for any client address (/, /n/...), the rest is /assets/....
//
// Development: `npm run dev` is Vite with hot reload, API and fonts are
// proxied to the notes server (by default the test environment
// tools/test-env.sh, :8432; another one - NOTES_API=http://127.0.0.1:8421 npm run dev).
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";
import { balukCss } from "./src/baluk-css/plugin.ts";

const api = process.env.NOTES_API ?? "http://127.0.0.1:8432";

export default defineConfig({
  // The note look: assets/baluk.css from the src/baluk-css/ blocks.
  plugins: [svelte(), balukCss()],
  build: { outDir: "dist", emptyOutDir: true, assetsDir: "assets", target: "es2023" },
  server: {
    port: 5173,
    strictPort: true,
    proxy: { "/api": api, "/fonts": api },
    // Rendering snapshots: the reference for the formula parser tests (lib/plot).
    fs: { allow: [".", "../tests/snapshots"] },
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "happy-dom",
  },
});
