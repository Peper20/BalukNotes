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
}

const glue = { default: async () => {}, WasmCore: FakeWasm } as unknown as Glue;

const SEED: Record<string, Seed> = {
  "/files.json": { vault: [["a.typ", b64("= A")]], packages: [["preview/p/1.0.0/lib.typ", b64("#let x = 1")]] },
  "/packages.json": { packages: [["preview/p/1.0.0/lib.typ", b64("#let x = 1")]] },
};

function env(root: FakeDir | null): { env: Env; fetched: string[] } {
  const fetched: string[] = [];
  return {
    fetched,
    env: {
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
  // The bundle's packages are used (not the vault seed), and not written back.
  expect(fetched).toEqual(["/packages.json"]);
  expect([...wasm.packageFiles.keys()]).toEqual(["preview/p/1.0.0/lib.typ"]);

  // A request writes a file into the placeholder and changes a setting.
  wasm.write("x.typ", "= x");
  wasm.settingsChanged = true;
  wasm.settingsValue = '{"appearance.font_size":23}';
  await core.flush?.();
  expect(root.tree()).toEqual({ ...before, "notes/settings.json": '{"appearance.font_size":23}' });
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
