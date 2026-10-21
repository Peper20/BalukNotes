// `test` of the scenarios that use the app server: every Playwright worker has
// its OWN `notes-typst serve` (port 8600 + parallelIndex, e2e/ports.ts) on its
// own copy of the warmed template data (e2e/global-setup.ts): the test vault,
// the disk cache, the trash. The scenarios of one worker run in order (a file is
// not split), the workers cannot disturb each other. `baseURL` is the worker's
// server. Specs with servers of their own (auth, sync, hub) use `test` of
// @playwright/test.
import { cpSync, rmSync } from "node:fs";
import { test as base } from "@playwright/test";
import { MAX_WORKERS, appPort } from "./ports";
import { TEMPLATE_DATA, exitOf, startApp, stop, waitFor, workerData } from "./server";

export { expect } from "@playwright/test";

export const test = base.extend<object, { app: { url: string } }>({
  app: [
    // eslint-disable-next-line no-empty-pattern
    async ({}, use, worker) => {
      const index = worker.parallelIndex;
      if (index >= MAX_WORKERS) throw new Error(`at most ${MAX_WORKERS} workers: no port for the worker ${index}`);
      const data = workerData(index);
      rmSync(data, { recursive: true, force: true });
      // The times are kept: a note's cache record is valid for the same file sizes and times.
      cpSync(TEMPLATE_DATA, data, { recursive: true, preserveTimestamps: true });
      const port = appPort(index);
      const server = startApp(data, port);
      const url = `http://127.0.0.1:${port}`;
      try {
        await waitFor(`${url}/api/vaults`, 120_000, false, exitOf(server));
        await use({ url });
      } finally {
        await stop(server);
      }
    },
    { scope: "worker", timeout: 150_000 },
  ],
  baseURL: async ({ app }, use) => use(app.url),
});
