// Плагин Vite: `assets/baluk.css` из файлов блоков (`bundle.ts`) — в сборке
// файлом `dist/assets/baluk.css`, в `npm run dev` — по тому же адресу.

import type { Plugin } from "vite";
import { bundle } from "./bundle";

export const BALUK_CSS = "assets/baluk.css";

export function balukCss(): Plugin {
  return {
    name: "baluk-css",
    async generateBundle() {
      const seen = new Set<string>();
      const source = await bundle(undefined, seen);
      for (const f of seen) this.addWatchFile(f);
      this.emitFile({ type: "asset", fileName: BALUK_CSS, source });
    },
    configureServer(server) {
      server.middlewares.use(`/${BALUK_CSS}`, (_req, res, next) => {
        bundle().then(
          (css) => {
            res.setHeader("Content-Type", "text/css; charset=utf-8");
            res.setHeader("Cache-Control", "no-cache");
            res.end(css);
          },
          (e: unknown) => next(e),
        );
      });
    },
  };
}
