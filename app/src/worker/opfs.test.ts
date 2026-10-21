import { afterEach, expect, it, vi } from "vitest";
import { importFiles, isStored, Persistence, readStored, type Dir, type FileHandle, type Source } from "./opfs";

const enc = new TextEncoder();
const dec = new TextDecoder();

const notFound = () => new DOMException("not found", "NotFoundError");

/** The subset of `FileSystemDirectoryHandle` the persistence uses, in memory. */
class FakeDir implements Dir {
  readonly kind = "directory";
  readonly dirs = new Map<string, FakeDir>();
  readonly files = new Map<string, Uint8Array>();
  /** Paths (as `dir/name`) whose write fails while listed. */
  failing = new Set<string>();

  constructor(
    readonly name = "",
    private readonly root: FakeDir | null = null,
  ) {}

  async getDirectoryHandle(name: string, { create = false } = {}): Promise<Dir> {
    let dir = this.dirs.get(name);
    if (!dir) {
      if (!create) throw notFound();
      dir = new FakeDir(name, this.root ?? this);
      this.dirs.set(name, dir);
    }
    return dir;
  }

  async getFileHandle(name: string, { create = false } = {}): Promise<FileHandle> {
    if (!this.files.has(name)) {
      if (!create) throw notFound();
      this.files.set(name, new Uint8Array());
    }
    return {
      kind: "file",
      name,
      getFile: async () => ({ arrayBuffer: async () => (this.files.get(name) as Uint8Array).slice().buffer }),
      createWritable: async () => ({
        write: async (data) => {
          if ((this.root ?? this).failing.has(name)) throw new Error("quota");
          this.files.set(name, data.slice());
        },
        close: async () => {},
      }),
    };
  }

  async removeEntry(name: string): Promise<void> {
    if (!this.files.delete(name) && !this.dirs.delete(name)) throw notFound();
  }

  async *values(): AsyncIterable<Dir | FileHandle> {
    for (const dir of this.dirs.values()) yield dir;
    for (const name of this.files.keys()) yield await this.getFileHandle(name);
  }

  /** Every file below, as `path -> text`. */
  tree(prefix = ""): Record<string, string> {
    const out: Record<string, string> = {};
    for (const [name, data] of this.files) out[prefix + name] = dec.decode(data);
    for (const [name, dir] of this.dirs) Object.assign(out, dir.tree(`${prefix}${name}/`));
    return out;
  }
}

/** A core with files in a map and a list of changes to hand out. */
class FakeCore implements Source {
  files = new Map<string, string>();
  changed: string[] = [];
  settings = false;
  json = "{}";

  set(path: string, text: string): void {
    this.files.set(path, text);
    this.changed.push(path);
  }
  remove(path: string): void {
    this.files.delete(path);
    this.changed.push(path);
  }
  takeChanges(): { files: string[]; settings: boolean } {
    const out = { files: this.changed, settings: this.settings };
    this.changed = [];
    this.settings = false;
    return out;
  }
  readFile(path: string): Uint8Array | undefined {
    const text = this.files.get(path);
    return text === undefined ? undefined : enc.encode(text);
  }
  settingsJson(): string {
    return this.json;
  }
}

const files = (list: [string, string][]): [string, Uint8Array][] => list.map(([p, t]) => [p, enc.encode(t)]);

afterEach(() => vi.restoreAllMocks());

it("imports the seed, marks it ready and reads it back", async () => {
  const root = new FakeDir();
  expect(await isStored(root, "v")).toBe(false);
  await importFiles(root, "v", {
    vault: files([["Книга/main.typ", "= Книга"], [".baluk/settings.json", "{}"]]),
    packages: files([["preview/p/1.0.0/lib.typ", "#let x = 1"]]),
  });
  expect(root.tree()).toEqual({
    "notes/vaults/v/files/Книга/main.typ": "= Книга",
    "notes/vaults/v/files/.baluk/settings.json": "{}",
    "notes/packages/preview/p/1.0.0/lib.typ": "#let x = 1",
    "notes/vaults/v/ready": "",
  });
  expect(await isStored(root, "v")).toBe(true);
  expect(await isStored(root, "other")).toBe(false);

  const kept = await readStored(root, "v");
  expect(kept.vault.map(([p, d]) => [p, dec.decode(d)]).sort()).toEqual([
    [".baluk/settings.json", "{}"],
    ["Книга/main.typ", "= Книга"],
  ]);
  expect(kept.packages.map(([p]) => p)).toEqual(["preview/p/1.0.0/lib.typ"]);
  expect(kept.settings).toBeNull();
});

it("an interrupted import (no ready file) is not a stored vault and is replaced by the next one", async () => {
  const root = new FakeDir();
  await (await root.getDirectoryHandle("notes", { create: true })).getDirectoryHandle("vaults", { create: true });
  const vaults = (await (await root.getDirectoryHandle("notes")).getDirectoryHandle("vaults")) as FakeDir;
  const half = (await vaults.getDirectoryHandle("v", { create: true })) as FakeDir;
  await (await half.getDirectoryHandle("files", { create: true })).getFileHandle("stale.typ", { create: true });
  expect(await isStored(root, "v")).toBe(false);
  await importFiles(root, "v", { vault: files([["a.typ", "a"]]), packages: [] });
  expect(root.tree()).toEqual({ "notes/vaults/v/files/a.typ": "a", "notes/vaults/v/ready": "" });
});

it("flush writes, rewrites and removes the changed files and the settings", async () => {
  const root = new FakeDir();
  const core = new FakeCore();
  const persistence = new Persistence(root, "v", core);

  core.set("a.typ", "a");
  core.set("Книга/1.typ", "1");
  core.set(".baluk/settings.json", '{"x":1}');
  core.settings = true;
  core.json = '{"theme":"dark"}';
  await persistence.flush();
  expect(root.tree()).toEqual({
    "notes/vaults/v/files/a.typ": "a",
    "notes/vaults/v/files/Книга/1.typ": "1",
    "notes/vaults/v/files/.baluk/settings.json": '{"x":1}',
    "notes/settings.json": '{"theme":"dark"}',
  });
  expect((await readStored(root, "v")).settings).toBe('{"theme":"dark"}');

  // A rewrite, a rename (both paths) and a removal.
  core.set("a.typ", "aa");
  core.remove("Книга/1.typ");
  core.set("Том/1.typ", "1");
  core.remove("never-existed.typ");
  await persistence.flush();
  expect(root.tree()).toEqual({
    "notes/vaults/v/files/a.typ": "aa",
    "notes/vaults/v/files/Том/1.typ": "1",
    "notes/vaults/v/files/.baluk/settings.json": '{"x":1}',
    "notes/settings.json": '{"theme":"dark"}',
  });

  // Nothing changed: nothing written (the settings file stays).
  root.failing.add("a.typ");
  await persistence.flush();
});

it("flush keeps a failed path and the settings for the next flush, with the latest content", async () => {
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  const root = new FakeDir();
  const core = new FakeCore();
  const persistence = new Persistence(root, "v", core);

  root.failing.add("bad.typ");
  root.failing.add("settings.json");
  core.set("ok.typ", "ok");
  core.set("bad.typ", "1");
  core.settings = true;
  await persistence.flush();
  // The failed writes left empty new files, as a real OPFS does.
  expect(root.tree()).toEqual({ "notes/vaults/v/files/ok.typ": "ok", "notes/vaults/v/files/bad.typ": "", "notes/settings.json": "" });
  expect(error).toHaveBeenCalledTimes(2);

  // The disk recovers; the core reports nothing new, the file changed meanwhile.
  root.failing.clear();
  core.files.set("bad.typ", "2");
  core.json = '{"a":2}';
  await persistence.flush();
  expect(root.tree()).toEqual({
    "notes/vaults/v/files/ok.typ": "ok",
    "notes/vaults/v/files/bad.typ": "2",
    "notes/settings.json": '{"a":2}',
  });

  // A path that was retried and then removed in the core is removed.
  root.failing.add("gone.typ");
  core.set("gone.typ", "x");
  await persistence.flush();
  root.failing.clear();
  core.remove("gone.typ");
  await persistence.flush();
  expect(Object.keys(root.tree())).not.toContain("notes/vaults/v/files/gone.typ");
  error.mockClear();
  await persistence.flush();
  expect(error).not.toHaveBeenCalled();
});

it("discard forgets the changes of the load", async () => {
  const root = new FakeDir();
  const core = new FakeCore();
  const persistence = new Persistence(root, "v", core);
  core.set("a.typ", "a");
  persistence.discard();
  await persistence.flush();
  expect(root.tree()).toEqual({});
});
