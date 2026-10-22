import { expect, it, vi } from "vitest";
import { FakeDir, files } from "./fakes";
import { StartRefused } from "./host";
import { load, PLACEHOLDER_VAULT, type Env, type Glue, type Seed } from "./load";
import { importFiles, readStored } from "./opfs";

const enc = new TextEncoder();
const dec = new TextDecoder();
const b64 = (text: string): string => btoa(text);

/** A fake `WasmCore` that records what it is given. */
class FakeWasm {
  static created: FakeWasm[] = [];
  vaultFiles = new Map<string, string>();
  packageFiles = new Map<string, string>();
  fonts: string[] = [];
  settings: string | null = null;
  opened = false;
  changed: string[] = [];
  settingsChanged = false;
  settingsValue = "{}";

  constructor(readonly vault: string) {
    if (vault.includes("/")) throw new Error(`a vault name cannot contain "/"`);
    FakeWasm.created.push(this);
  }
  addFile(path: string, data: Uint8Array): void {
    this.vaultFiles.set(path, dec.decode(data));
  }
  addPackageFile(path: string, data: Uint8Array): void {
    this.packageFiles.set(path, dec.decode(data));
  }
  addFont(data: Uint8Array): void {
    if (this.opened) throw new Error("fonts are added before open");
    this.fonts.push(dec.decode(data));
  }
  setSettings(json: string): void {
    this.settings = json;
  }
  open(): void {
    this.opened = true;
  }
  async request(): Promise<{ status: number; contentType: string; body: Uint8Array }> {
    return { status: 200, contentType: "text/plain", body: new Uint8Array() };
  }
  // Writes through the core, as a request would.
  write(path: string, text: string): void {
    this.vaultFiles.set(path, text);
    this.changed.push(path);
  }
  takeChanges(): { files: string[]; settings: boolean } {
    const out = { files: this.changed, settings: this.settingsChanged };
    this.changed = [];
    this.settingsChanged = false;
    return out;
  }
  readFile(path: string): Uint8Array | undefined {
    const text = this.vaultFiles.get(path);
    return text === undefined ? undefined : enc.encode(text);
  }
  settingsJson(): string {
    return this.settingsValue;
  }
  // Sync: each round runs the next of `rounds` (what it does to the vault) and returns its JSON.
  rounds: (() => object)[] = [];
  hubs: string[] = [];
  syncState: string | undefined = undefined;
  keptState: string | null = null;
  removed: [string, Uint8Array][] = [];
  sync(hub: string): string {
    this.hubs.push(hub);
    const round = this.rounds.shift();
    return JSON.stringify(round ? round() : { ok: true, changed: false });
  }
  setSyncState(json: string): void {
    if (!json.startsWith("{")) throw new Error("not a state");
    this.keptState = json;
  }
  takeSyncState(): string | undefined {
    const out = this.syncState;
    this.syncState = undefined;
    return out;
  }
  takeRemoved(): [string, Uint8Array][] {
    const out = this.removed;
    this.removed = [];
    return out;
  }
}

const glue = { default: async () => {}, WasmCore: FakeWasm } as unknown as Glue;

const SEED: Record<string, Seed> = {
  "/files.json": { vault: [["a.typ", b64("= A")]], packages: [["preview/p/1.0.0/lib.typ", b64("#let x = 1")]] },
  "/packages.json": { packages: [["preview/p/1.0.0/lib.typ", b64("#let x = 1")]] },
  // A later bundle: one more version.
  "/packages-2.json": {
    packages: [
      ["preview/p/1.0.0/lib.typ", b64("#let x = 1")],
      ["preview/p/2.0.0/lib.typ", b64("#let x = 2")],
    ],
  },
};

/** What `fetchBytes` serves: `fonts.json` and the font files it lists. */
const BYTES: Record<string, string> = {
  "/wasm/fonts.json": JSON.stringify({ fonts: ["fonts/font-aaaaaaaa.bin", "fonts/font-bbbbbbbb.bin"] }),
  "/wasm/fonts/font-aaaaaaaa.bin": "font A",
  "/wasm/fonts/font-bbbbbbbb.bin": "font B",
  "https://hub.example/w/fonts.json": JSON.stringify({ fonts: ["fonts/font-cccccccc.bin"] }),
  "https://hub.example/w/fonts/font-cccccccc.bin": "font C",
  "/broken/fonts.json": JSON.stringify({ fonts: ["fonts/font-aaaaaaaa.bin", "fonts/missing.bin"] }),
  "/broken/fonts/font-aaaaaaaa.bin": "font A",
};

function env(root: FakeDir | null): { env: Env; fetched: string[] } {
  const fetched: string[] = [];
  return {
    fetched,
    env: {
      fetchBytes: async (address) => {
        fetched.push(address);
        const data = BYTES[address];
        if (data === undefined) throw new Error(`${address}: 404 Not Found`);
        return enc.encode(data);
      },
      importGlue: async () => glue,
      opfsRoot: async () => root,
      fetchSeed: async (address) => {
        fetched.push(address);
        const seed = SEED[address];
        if (!seed) throw new Error(`${address}: 404`);
        return seed;
      },
    },
  };
}

const lastCore = (): FakeWasm => FakeWasm.created.at(-1)!;

it("runs the named vault: seeds it into OPFS once, then reads it back from OPFS", async () => {
  const root = new FakeDir();
  const first = env(root);
  const core = await load({ module: "/m.js", vault: "Учёба", files: "/files.json" }, first.env);
  expect(lastCore().vault).toBe("Учёба");
  expect(lastCore().vaultFiles.get("a.typ")).toBe("= A");
  expect(lastCore().opened).toBe(true);
  expect(root.tree()).toMatchObject({ "notes/vaults/Учёба/files/a.typ": "= A", "notes/vaults/Учёба/ready": "" });

  lastCore().write("b.typ", "= B");
  await core.flush?.();
  expect(root.tree()["notes/vaults/Учёба/files/b.typ"]).toBe("= B");

  const second = env(root);
  await load({ module: "/m.js", vault: "Учёба", files: "/files.json" }, second.env);
  expect([...lastCore().vaultFiles.keys()].sort()).toEqual(["a.typ", "b.typ"]);
  expect(second.fetched).toEqual([]);
});

it("hands the fonts of fonts.json to the core before it opens, resolved against the address of the list", async () => {
  const first = env(null);
  await load({ module: "/m.js", vault: "Учёба", fonts: "/wasm/fonts.json" }, first.env);
  expect(lastCore().fonts).toEqual(["font A", "font B"]);
  expect(first.fetched).toEqual(["/wasm/fonts.json", "/wasm/fonts/font-aaaaaaaa.bin", "/wasm/fonts/font-bbbbbbbb.bin"]);
  expect(lastCore().opened).toBe(true);

  await load({ module: "/m.js", fonts: "https://hub.example/w/fonts.json" }, env(null).env);
  expect(lastCore().fonts).toEqual(["font C"]);

  await load({ module: "/m.js", vault: "Учёба" }, env(null).env);
  expect(lastCore().fonts).toEqual([]);
});

it("a font that cannot be fetched fails the start", async () => {
  await expect(load({ module: "/m.js", vault: "Учёба", fonts: "/broken/fonts.json" }, env(null).env)).rejects.toThrow("/broken/fonts/missing.bin: 404");
  expect(lastCore().opened).toBe(false);
  await expect(load({ module: "/m.js", vault: "Учёба", fonts: "/nowhere/fonts.json" }, env(null).env)).rejects.toThrow("/nowhere/fonts.json: 404");
});

it("without a vault runs the placeholder: nothing of it is read from or written to OPFS, the settings and the packages are kept", async () => {
  const root = new FakeDir();
  // An earlier run kept a vault with the placeholder's own name: the placeholder must not see it.
  await importFiles(root, PLACEHOLDER_VAULT, { vault: files([["secret.typ", "= secret"]]), packages: [] });
  const before = { ...root.tree() };

  const { env: e, fetched } = env(root);
  const core = await load({ module: "/m.js", files: "/files.json", packages: "/packages.json" }, e);
  const wasm = lastCore();
  expect(wasm.vault).toBe(PLACEHOLDER_VAULT);
  expect([...wasm.vaultFiles.keys()]).toEqual([]);
  expect(wasm.opened).toBe(true);
  // The bundle's packages are used (not the vault seed) and kept.
  expect(fetched).toEqual(["/packages.json"]);
  expect([...wasm.packageFiles.keys()]).toEqual(["preview/p/1.0.0/lib.typ"]);

  // A request writes a file into the placeholder and changes a setting.
  wasm.write("x.typ", "= x");
  wasm.settingsChanged = true;
  wasm.settingsValue = '{"appearance.font_size":23}';
  await core.flush?.();
  expect(root.tree()).toEqual({
    ...before,
    "notes/packages/preview/p/1.0.0/lib.typ": "#let x = 1",
    "notes/settings.json": '{"appearance.font_size":23}',
  });
});

it("the settings kept by the picker screen are used by a vault opened for the first time", async () => {
  const root = new FakeDir();
  const placeholder = await load({ module: "/m.js", packages: "/packages.json" }, env(root).env);
  const wasm = lastCore();
  wasm.settingsChanged = true;
  wasm.settingsValue = '{"theme":"dark"}';
  await placeholder.flush?.();

  await load({ module: "/m.js", vault: "Новое", packages: "/packages.json" }, env(root).env);
  expect(lastCore().settings).toBe('{"theme":"dark"}');
  expect(lastCore().opened).toBe(true);
  // The vault is imported empty and marked ready; its packages come from the bundle.
  expect(root.tree()).toMatchObject({ "notes/vaults/Новое/ready": "", "notes/packages/preview/p/1.0.0/lib.typ": "#let x = 1" });
  expect((await readStored(root, "Новое")).vault).toEqual([]);
});

it("the placeholder starts without OPFS and without a bundle", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const core = await load({ module: "/m.js" }, env(null).env);
  expect(lastCore().vault).toBe(PLACEHOLDER_VAULT);
  await core.flush?.();
});

it("a vault name the core refuses is StartRefused with the core's message", async () => {
  await expect(load({ module: "/m.js", vault: "a/b" }, env(new FakeDir()).env)).rejects.toThrow(StartRefused);
  await expect(load({ module: "/m.js", vault: "a/b" }, env(new FakeDir()).env)).rejects.toThrow('a vault name cannot contain "/"');
});

it("a vault of the hub starts empty, is complete in OPFS after its first round, and comes back from OPFS", async () => {
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  const root = new FakeDir();
  // An interrupted first round left a file and a state, but no `ready`.
  await importFiles(root, "Учёба", { vault: files([["stale.typ", "= old"]]), packages: [] });
  await (await (await (await root.getDirectoryHandle("notes")).getDirectoryHandle("vaults")).getDirectoryHandle("Учёба")).removeEntry("ready");

  const source = { module: "/m.js", vault: "Учёба", packages: "/packages.json", hub: "" };
  const core = await load(source, env(root).env);
  const wasm = lastCore();
  expect([...wasm.vaultFiles.keys()]).toEqual([]);
  expect([...wasm.packageFiles.keys()]).toEqual(["preview/p/1.0.0/lib.typ"]);
  expect(Object.keys(root.tree()).filter((p) => p.startsWith("notes/vaults/"))).toEqual([]);
  expect(core.sync).toBeDefined();

  // A failed round (offline): the vault is not complete yet.
  wasm.rounds.push(() => ({ ok: false, error: "network error: no answer", held: false, unauthorized: false }));
  expect(await core.sync!()).toBe(false);
  expect(error).toHaveBeenCalledWith(expect.stringContaining("no answer"));
  expect(root.tree()["notes/vaults/Учёба/ready"]).toBeUndefined();

  // The first round that completes: the pulled files, the state, the removed content, then `ready`.
  wasm.rounds.push(() => {
    wasm.write("a.typ", "= A");
    wasm.syncState = '{"remote_seq":1,"files":{}}';
    wasm.removed = files([["old.typ", "= removed"]]);
    return { ok: true, changed: true };
  });
  expect(await core.sync!()).toBe(true);
  expect(wasm.hubs).toEqual(["", ""]);
  expect(root.tree()).toMatchObject({
    "notes/vaults/Учёба/files/a.typ": "= A",
    "notes/vaults/Учёба/sync.json": '{"remote_seq":1,"files":{}}',
    "notes/vaults/Учёба/removed/old.typ": "= removed",
    "notes/vaults/Учёба/ready": "",
  });
  // A round that pulled nothing.
  expect(await core.sync!()).toBe(false);

  await load(source, env(root).env);
  expect([...lastCore().vaultFiles.keys()]).toEqual(["a.typ"]);
  expect(lastCore().keptState).toBe('{"remote_seq":1,"files":{}}');
});

it("a held round is logged and does not complete the vault", async () => {
  const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
  const root = new FakeDir();
  const core = await load({ module: "/m.js", vault: "v", packages: "/packages.json", hub: "" }, env(root).env);
  lastCore().rounds.push(() => ({ ok: false, error: "5 of 5 files are gone", held: true, unauthorized: false }));
  expect(await core.sync!()).toBe(false);
  expect(warn).toHaveBeenCalledWith(expect.stringContaining("held"));
  expect(root.tree()["notes/vaults/v/ready"]).toBeUndefined();
});

it("the sync state is written only once every vault file is", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const root = new FakeDir();
  const core = await load({ module: "/m.js", vault: "v", packages: "/packages.json", hub: "" }, env(root).env);
  const wasm = lastCore();
  root.failing.add("b.typ");
  wasm.rounds.push(() => {
    wasm.write("b.typ", "= B");
    wasm.syncState = '{"remote_seq":2,"files":{}}';
    return { ok: true, changed: true };
  });
  await core.sync!();
  expect(root.tree()["notes/vaults/v/sync.json"]).toBeUndefined();
  expect(root.tree()["notes/vaults/v/ready"]).toBeUndefined();
  root.failing.delete("b.typ");
  await core.flush!();
  expect(root.tree()).toMatchObject({ "notes/vaults/v/files/b.typ": "= B", "notes/vaults/v/sync.json": '{"remote_seq":2,"files":{}}' });
});

it("a kept vault gets the package versions a later bundle adds; without the list it goes with the kept ones", async () => {
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  const root = new FakeDir();
  await load({ module: "/m.js", vault: "v", files: "/files.json" }, env(root).env);
  expect(Object.keys(root.tree()).filter((p) => p.startsWith("notes/packages/"))).toEqual(["notes/packages/preview/p/1.0.0/lib.typ"]);

  await load({ module: "/m.js", vault: "v", files: "/files.json", packages: "/packages-2.json" }, env(root).env);
  expect([...lastCore().packageFiles.keys()].sort()).toEqual(["preview/p/1.0.0/lib.typ", "preview/p/2.0.0/lib.typ"]);
  expect(root.tree()["notes/packages/preview/p/2.0.0/lib.typ"]).toBe("#let x = 2");

  await load({ module: "/m.js", vault: "v", files: "/files.json", packages: "/missing.json" }, env(root).env);
  expect([...lastCore().packageFiles.keys()].sort()).toEqual(["preview/p/1.0.0/lib.typ", "preview/p/2.0.0/lib.typ"]);
  expect(error).toHaveBeenCalledWith(expect.stringContaining("/missing.json"));
});

it("reports the size of the module's memory when the glue exports it, and its growth", async () => {
  const memory = { buffer: { byteLength: 1000 } };
  const withMemory = { default: async () => ({ memory }), WasmCore: FakeWasm } as unknown as Glue;
  const core = await load({ module: "/m.js", vault: "v" }, { ...env(null).env, importGlue: async () => withMemory });
  expect(core.memoryBytes?.()).toBe(1000);
  memory.buffer = { byteLength: 5000 };
  expect(core.memoryBytes?.()).toBe(5000);

  // Without the export (the glue's init resolves to nothing) the memory is not checked.
  expect((await load({ module: "/m.js", vault: "v" }, env(null).env)).memoryBytes).toBeUndefined();
});
