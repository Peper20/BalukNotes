// `test` of the scenarios that use the app server: every Playwright worker has
// its OWN `notes-typst serve` (port 8600 + parallelIndex, e2e/ports.ts) on its
// own copy of the warmed template data (tests/.data/e2e/w<index>: the test vault,
// the disk cache, its own trash; e2e/global-setup.ts): all notes are built, so the
// waits are short (01-switch changes the files itself to get a first build). The
// scenarios of one worker run in order (a file is not split), the workers cannot
// disturb each other. `baseURL` is the worker's server. Specs with servers of
// their own (auth, sync, hub) use `test` of @playwright/test.
import { test as base } from "@playwright/test";
import { VAULT_NAME } from "./helpers";
import { MAX_WORKERS, appPort } from "./ports";
import { exitOf, prepareData, startApp, stop, waitFor, warm, workerData } from "./server";

export { expect } from "@playwright/test";

export const test = base.extend<object, { app: { url: string } }>({
  app: [
    // eslint-disable-next-line no-empty-pattern
    async ({}, use, worker) => {
      const index = worker.parallelIndex;
      if (index >= MAX_WORKERS) throw new Error(`at most ${MAX_WORKERS} workers: no port for the worker ${index}`);
      const data = workerData(index);
      const port = appPort(index);
      prepareData(data, VAULT_NAME);
      const server = startApp(data, port);
      const url = `http://127.0.0.1:${port}`;
      try {
        await waitFor(`${url}/api/vaults`, 120_000, false, exitOf(server));
        // With the template's cache every note is a disk read: the line shows whether it was.
        const started = Date.now();
        const count = await warm(url, VAULT_NAME);
        console.log(`e2e worker ${index}: ${count} notes ready in ${((Date.now() - started) / 1000).toFixed(1)} s`);
        await use({ url });
      } finally {
        await stop(server);
      }
    },
    { scope: "worker", timeout: 300_000 },
  ],
  baseURL: async ({ app }, use) => use(app.url),
});
