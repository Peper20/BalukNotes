// Starts the core of a worker: the module, then the vault, the packages and the
// settings from OPFS or the seed. Plain TypeScript with the browser's parts
// passed in (`Env`; `core.ts` wires the real ones), so it is tested with fakes.
// A vault of the hub (`CoreSource.hub`) gets a sync round (`Core.sync`, run
// by the host's queue); its files come from the hub, not from a seed.

import { StartRefused, type Core } from "./host";
import { clearVault, importFiles, isStored, markReady, Persistence, readStored, writePackages, type Dir, type Files, type Source, type Stored } from "./opfs";
import type { CoreSource } from "./protocol";

/** The name of the core's vault while the page names none (the picker screen). It is never read from or written to OPFS. */
export const PLACEHOLDER_VAULT = "placeholder";

/** What `notes_wasm.js` exports (wasm-bindgen, `--target web`). */
export interface Glue {
  /** The wasm-bindgen init; resolves to the module's exports (`memory` among them). */
  default(): Promise<{ memory?: { buffer: { byteLength: number } } } | undefined>;
  WasmCore: new (vault: string) => Core &
    Source & {
      addFile(path: string, data: Uint8Array): void;
      addPackageFile(path: string, data: Uint8Array): void;
      /** A font file of the core's font set, before `open`. */
      addFont(data: Uint8Array): void;
      setSettings(json: string): void;
      open(): void;
      /** One sync round with the hub at `hub`, blocking: `Round` JSON. */
      sync(hub: string): string;
      setSyncState(json: string): void;
      takeSyncState(): string | undefined;
      takeRemoved(): Files;
    };
}

/** What a sync round says (`notes_wasm::round_json`). */
export type Round =
  | { ok: true; changed: boolean; uploaded: number; downloaded: number; removedLocal: number; removedRemote: number; conflicts: string[]; skipped: string[] }
  | { ok: false; error: string; held: boolean; unauthorized: boolean };

/** `fonts.json` as it is served: the font files, as paths relative to its address. */
export interface FontList {
  fonts: string[];
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
  /** The bytes at `address`; throws if it cannot be had (a non-OK answer). */
  fetchBytes(address: string): Promise<Uint8Array>;
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
 * Hands the kept package files to the core, then, with `address` set, the
 * whitelisted ones of the bundle that OPFS lacks (a later bundle may ship new
 * versions; a version never changes in place, so only the paths are
 * compared), and keeps those in OPFS. With nothing kept the bundle's list is
 * required; otherwise a failure to get it is only logged.
 */
async function addPackages(core: { addPackageFile(path: string, data: Uint8Array): void }, kept: Files, address: string | undefined, env: Env, root: Dir | null): Promise<void> {
  for (const [path, data] of kept) core.addPackageFile(path, data);
  if (!address) return;
  let bundled: Files;
  try {
    bundled = (await seedFrom(env, address)).packages;
  } catch (e) {
    if (kept.length === 0) throw e;
    console.error(`packages: the bundle's list is not available, going with the kept ones: ${describe(e)}`);
    return;
  }
  const have = new Set(kept.map(([path]) => path));
  const missing = bundled.filter(([path]) => !have.has(path));
  for (const [path, data] of missing) core.addPackageFile(path, data);
  if (root && missing.length > 0) {
    try {
      await writePackages(root, missing);
    } catch (e) {
      console.error(`OPFS: cannot save the packages: ${describe(e)}`);
    }
  }
}

/**
 * Hands the core its fonts: the module carries none. The files of the list at
 * `address` are fetched (the browser's HTTP cache keeps them between loads;
 * their names carry a hash of the content) and given to the core in the order
 * of the list. A font that cannot be had fails the start: the core cannot
 * render the way a vault needs without them.
 */
async function addFonts(core: { addFont(data: Uint8Array): void }, address: string, env: Env): Promise<void> {
  const { fonts } = JSON.parse(new TextDecoder().decode(await env.fetchBytes(address))) as FontList;
  const base = address.slice(0, address.lastIndexOf("/") + 1);
  const data = await Promise.all(fonts.map((path) => env.fetchBytes(base + path)));
  for (const font of data) core.addFont(font);
}

/**
 * Starts the core for `source`. The vault is `source.vault`; unset, the core
 * runs an empty placeholder vault that OPFS never sees (the shared settings and
 * the packages are still kept). A vault name the core refuses is `StartRefused`.
 * With `hub` the named vault syncs with the hub: OPFS is its source once the
 * first round completed (`ready`), before that it starts empty.
 */
export async function load({ module, vault, files, packages, fonts, hub }: CoreSource, env: Env): Promise<Core> {
  const glue = await env.importGlue(module);
  const exports = await glue.default();
  let core: InstanceType<Glue["WasmCore"]>;
  try {
    core = new glue.WasmCore(vault ?? PLACEHOLDER_VAULT);
  } catch (e) {
    throw new StartRefused(describe(e));
  }
  const key = vault ?? null;
  const syncing = hub !== undefined && key !== null;
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
  let ready = kept?.ready ?? false;
  if (ready || key === null) {
    for (const [path, data] of kept?.stored.vault ?? []) core.addFile(path, data);
    if (syncing && kept?.stored.sync != null) {
      try {
        core.setSyncState(kept.stored.sync);
      } catch (e) {
        // An empty state compares the whole vault with the hub: nothing is deleted.
        console.error(`OPFS: the kept sync state is not used: ${describe(e)}`);
      }
    }
    await addPackages(core, kept?.stored.packages ?? [], packages, env, root);
  } else if (syncing) {
    // A vault of the hub that OPFS does not have complete: whatever an
    // interrupted first round left goes, the first round fills it again.
    if (root) {
      try {
        await clearVault(root, key);
      } catch (e) {
        console.error(`OPFS: cannot clear the incomplete vault, nothing is kept this run: ${describe(e)}`);
        persistence = null;
      }
    }
    await addPackages(core, kept?.stored.packages ?? [], packages, env, root);
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
  if (fonts) await addFonts(core, fonts, env);
  core.open();
  // What was just loaded is in OPFS already (or not meant to be there).
  persistence?.discard();
  // Without OPFS the changes are still taken: they wake the waiting events.
  const flush = async (): Promise<boolean> => (persistence ? persistence.flush() : core.takeChanges().files.length > 0);
  const sync = async (): Promise<boolean> => {
    const round = JSON.parse(core.sync(hub as string)) as Round;
    if (!round.ok && round.held) console.warn(`sync of "${key}" is held, nothing is applied until it is confirmed: ${round.error}`);
    else if (!round.ok) console.error(`sync of "${key}": ${round.error}`);
    const changed = await flush();
    if (round.ok && !ready && root && persistence?.clean) {
      try {
        await markReady(root, key as string);
        ready = true;
      } catch (e) {
        console.error(`OPFS: cannot mark the vault complete: ${describe(e)}`);
      }
    }
    return changed;
  };
  return {
    request: (method, uri, body) => core.request(method, uri, body),
    flush,
    ...(exports?.memory && { memoryBytes: () => exports.memory!.buffer.byteLength }),
    ...(syncing && { sync }),
  };
}
