// The worker that runs the core: the `notes-wasm` module serves the API
// requests the client sends as messages (`lib/api/transport.ts`). Queue and
// failures - `host.ts`.

import { CoreHost, type Core } from "./host";
import { importFiles, isStored, Persistence, readStored, type Dir, type Files, type Source, type Stored } from "./opfs";
import { DEFAULT_VAULT, type CoreSource, type ResponseMessage, type WorkerMessage } from "./protocol";

/** The worker global scope; the DOM types of the project do not mix with `lib.webworker`. */
interface Scope {
  onmessage: ((e: MessageEvent<WorkerMessage>) => void) | null;
  postMessage(message: unknown, transfer: Transferable[]): void;
}
const scope = self as unknown as Scope;

/** What `notes_wasm.js` exports (wasm-bindgen, `--target web`). */
interface Glue {
  default(): Promise<unknown>;
  WasmCore: new (vault: string) => Core &
    Source & {
      addFile(path: string, data: Uint8Array): void;
      addPackageFile(path: string, data: Uint8Array): void;
      setSettings(json: string): void;
      open(): void;
    };
}

/** `files.json` and `packages.json` as they are served: base64 contents. */
interface Seed {
  vault?: [string, string][];
  packages?: [string, string][];
}

const bytes = (base64: string): Uint8Array => Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));

const decode = (list: [string, string][]): Files => list.map(([path, data]) => [path, bytes(data)]);

async function fetchSeed(address: string): Promise<{ vault: Files; packages: Files }> {
  const res = await fetch(address);
  if (!res.ok) throw new Error(`${address}: ${res.status} ${res.statusText}`);
  const seed = (await res.json()) as Seed;
  return { vault: decode(seed.vault ?? []), packages: decode(seed.packages ?? []) };
}

/** OPFS of the origin; `null` where the browser has none (the core then keeps nothing between loads). */
async function opfsRoot(): Promise<Dir | null> {
  try {
    return (await navigator.storage.getDirectory()) as unknown as Dir;
  } catch (e) {
    console.error(`OPFS is not available, nothing is kept between loads: ${e instanceof Error ? e.message : e}`);
    return null;
  }
}

/** What an earlier run kept; `null`: nothing, or it cannot be read (the vault is seeded again). */
async function readKept(root: Dir | null, vault: string): Promise<Stored | null> {
  try {
    return root && (await isStored(root, vault)) ? await readStored(root, vault) : null;
  } catch (e) {
    console.error(`OPFS: cannot read the kept vault, starting from the seed: ${e instanceof Error ? e.message : e}`);
    return null;
  }
}

async function load({ module, vault = DEFAULT_VAULT, files, packages }: CoreSource): Promise<Core> {
  const glue = (await import(/* @vite-ignore */ module)) as Glue;
  await glue.default();
  const core = new glue.WasmCore(vault);
  const root = await opfsRoot();
  let persistence: Persistence | null = root && new Persistence(root, vault, core);

  const kept = await readKept(root, vault);
  if (kept) {
    for (const [path, data] of kept.vault) core.addFile(path, data);
    // The packages of an earlier run, or - when it kept none - the bundle's (not written back).
    const keptPackages = kept.packages.length > 0 || !packages ? kept.packages : (await fetchSeed(packages)).packages;
    for (const [path, data] of keptPackages) core.addPackageFile(path, data);
    if (kept.settings !== null) {
      try {
        core.setSettings(kept.settings);
      } catch (e) {
        console.error(`OPFS: the kept settings are not used: ${e instanceof Error ? e.message : e}`);
      }
    }
  } else if (files || packages) {
    const seed = files ? await fetchSeed(files) : { vault: [], packages: [] };
    if (seed.packages.length === 0 && packages) seed.packages = (await fetchSeed(packages)).packages;
    for (const [path, data] of seed.vault) core.addFile(path, data);
    for (const [path, data] of seed.packages) core.addPackageFile(path, data);
    if (root) {
      try {
        await importFiles(root, vault, seed);
      } catch (e) {
        // A half-written copy has no `ready` file; this run keeps nothing, the next one seeds again.
        console.error(`OPFS: cannot save the vault: ${e instanceof Error ? e.message : e}`);
        persistence = null;
      }
    }
  }
  core.open();
  // What was just loaded is in OPFS already (or not meant to be there).
  persistence?.discard();
  return {
    request: (method, uri, body) => core.request(method, uri, body),
    flush: async () => persistence?.flush(),
  };
}

const host = new CoreHost(load, (answer: ResponseMessage) => scope.postMessage(answer, [answer.body.buffer]));

scope.onmessage = ({ data }) => {
  if ("start" in data) host.start(data.start);
  else host.request(data);
};
