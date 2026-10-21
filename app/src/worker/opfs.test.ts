import { afterEach, expect, it, vi } from "vitest";
import { FakeCore, FakeDir, files } from "./fakes";
import { importFiles, isStored, Persistence, readStored } from "./opfs";

const dec = new TextDecoder();

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
