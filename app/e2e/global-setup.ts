// Before the scenarios: builds the binaries once (the specs and the workers
// start them directly) and makes the template data of the app servers: a copy of
// the test vault on a server that builds all notes, so the waits of the scenarios
// are short. The workers start on copies of it (e2e/server.ts, `prepareData`);
// scenarios that need the first build (01-switch) change the note file themselves,
// which makes its cache stale.
import { join } from "node:path";
import { VAULT_NAME } from "./helpers";
import { WARM_PORT } from "./ports";
import { E2E_DATA, ROOT, TEMPLATE_DATA, buildBinaries, copyTree, exitOf, startApp, stop, waitFor, warm } from "./server";
import { rmSync } from "node:fs";

export default async function setup() {
  buildBinaries();
  rmSync(E2E_DATA, { recursive: true, force: true });
  copyTree(join(ROOT, "tests/vault"), join(TEMPLATE_DATA, "vaults", VAULT_NAME));
  const base = `http://127.0.0.1:${WARM_PORT}`;
  const server = startApp(TEMPLATE_DATA, WARM_PORT);
  try {
    await waitFor(`${base}/api/vaults`, 120_000, false, exitOf(server));
    const started = Date.now();
    const count = await warm(base, VAULT_NAME);
    console.log(`e2e template warmed: ${count} notes in ${((Date.now() - started) / 1000).toFixed(1)} s`);
  } finally {
    // Stopped before the copies are made: the disk cache is complete.
    await stop(server);
  }
}
