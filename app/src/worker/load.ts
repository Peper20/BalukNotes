// Starts the core of a worker: the module, then the vault, the packages and the
// settings from OPFS or the seed. Plain TypeScript with the browser's parts
// passed in (`Env`; `core.ts` wires the real ones), so it is tested with fakes.

import { StartRefused, type Core } from "./host";
import { importFiles, isStored, Persistence, readStored, type Dir, type Files, type Source, type Stored } from "./opfs";
import type { CoreSource } from "./protocol";

/** The name of the core's vault while the page names none (the picker screen). It is never read from or written to OPFS. */
export const PLACEHOLDER_VAULT = "placeholder";

/** What `notes_wasm.js` exports (wasm-bindgen, `--target web`). */
export interface Glue {
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
export interface Seed {
  vault?: [string, string][];
  packages?: [string, string][];
}

/** What the browser gives the loader. */
export interface Env {
  importGlue(address: string): Promise<Glue>;
  /** OPFS of the origin; `null` where the browser has none (the core then keeps nothing between loads). */
  opfsRoot(): Promise<Dir | null>;
  /** The JSON at `address`; throws if it cannot be had. */
  fetchSeed(address: string): Promise<Seed>;
}

const bytes = (base64: string): Uint8Array => Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));

const decode = (list: [string, string][]): Files => list.map(([path, data]) => [path, bytes(data)]);

const describe = (e: unknown): string => (e instanceof Error ? e.message : String(e));

async function seedFrom(env: Env, address: string): Promise<{ vault: Files; packages: Files }> {
  const seed = await env.fetchSeed(address);
  return { vault: decode(seed.vault ?? []), packages: decode(seed.packages ?? []) };
}

/**
 * What an earlier run kept; `null`: nothing, or it cannot be read (the vault is
 * seeded again). `ready` says the named vault was imported completely: only
 * then its files are the source, otherwise they are ignored. No vault (`key`
 * is `null`): only the packages and the settings.
 */
async function readKept(root: Dir | null, key: string | null): Promise<{ stored: Stored; ready: boolean } | null> {
  if (!root) return null;
  try {
    const ready = key !== null && (await isStored(root, key));
    return { stored: await readStored(root, ready ? key : null), ready };
  } catch (e) {
    console.error(`OPFS: cannot read the kept vault, starting from the seed: ${describe(e)}`);
    return null;
  }
}

/**
 * Starts the core for `source`. The vault is `source.vault`; unset, the core
 * runs an empty placeholder vault that OPFS never sees (the shared settings and
 * the packages are still kept). A vault name the core refuses is `StartRefused`.
 */
export async function load({ module, vault, files, packages }: CoreSource, env: Env): Promise<Core> {
  const glue = await env.importGlue(module);
  await glue.default();
  let core: InstanceType<Glue["WasmCore"]>;
  try {
    core = new glue.WasmCore(vault ?? PLACEHOLDER_VAULT);
  } catch (e) {
    throw new StartRefused(describe(e));
  }
  const key = vault ?? null;
  const root = await env.opfsRoot();
  let persistence: Persistence | null = root && new Persistence(root, key, core);

  const kept = await readKept(root, key);
  if (kept?.stored.settings != null) {
    try {
      core.setSettings(kept.stored.settings);
    } catch (e) {
      console.error(`OPFS: the kept settings are not used: ${describe(e)}`);
    }
  }
  if (kept?.ready || key === null) {
    for (const [path, data] of kept?.stored.vault ?? []) core.addFile(path, data);
    // The packages of an earlier run, or - when it kept none - the bundle's (not written back).
    const keptPackages = kept && (kept.stored.packages.length > 0 || !packages) ? kept.stored.packages : packages ? (await seedFrom(env, packages)).packages : [];
    for (const [path, data] of keptPackages) core.addPackageFile(path, data);
  } else if (files || packages) {
    const seed = files ? await seedFrom(env, files) : { vault: [], packages: [] };
    if (seed.packages.length === 0 && packages) seed.packages = (await seedFrom(env, packages)).packages;
    for (const [path, data] of seed.vault) core.addFile(path, data);
    for (const [path, data] of seed.packages) core.addPackageFile(path, data);
    if (root && key !== null) {
      try {
        await importFiles(root, key, seed);
      } catch (e) {
        // A half-written copy has no `ready` file; this run keeps nothing, the next one seeds again.
        console.error(`OPFS: cannot save the vault: ${describe(e)}`);
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
