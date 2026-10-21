// The core's files in the Origin Private File System (architecture §1): the
// vault lives in the core's memory, the worker writes what changed through to
// OPFS after each request and reads it all back at the start.
//
//   notes/vaults/<vault>/files/<path>                   vault files (.baluk/settings.json among them)
//   notes/vaults/<vault>/ready                          written last by the first import
//   notes/settings.json                                 shared settings values
//   notes/packages/<namespace>/<name>/<version>/<file>  Typst package files
//
// Only the subset of `FileSystemDirectoryHandle` used here is described
// (`Dir`), so the tests run over a fake directory.

export interface FileLike {
  arrayBuffer(): Promise<ArrayBuffer>;
}

export interface Writable {
  write(data: Uint8Array): Promise<void>;
  close(): Promise<void>;
}

export interface FileHandle {
  readonly kind: "file";
  readonly name: string;
  getFile(): Promise<FileLike>;
  createWritable(): Promise<Writable>;
}

export interface Dir {
  readonly kind: "directory";
  readonly name: string;
  getDirectoryHandle(name: string, options?: { create?: boolean }): Promise<Dir>;
  getFileHandle(name: string, options?: { create?: boolean }): Promise<FileHandle>;
  removeEntry(name: string, options?: { recursive?: boolean }): Promise<void>;
  values(): AsyncIterable<Dir | FileHandle>;
}

/** Files by path: the vault files, the package files. */
export type Files = [path: string, data: Uint8Array][];

/** What the persistence needs of `WasmCore`. */
export interface Source {
  takeChanges(): { files: string[]; settings: boolean };
  /** `undefined`: the file is gone. */
  readFile(path: string): Uint8Array | undefined;
  settingsJson(): string;
}

/** What the core is started with. */
export interface Stored {
  vault: Files;
  packages: Files;
  /** The shared settings JSON; `null`: none yet. */
  settings: string | null;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

const isNotFound = (e: unknown): boolean => e instanceof DOMException && e.name === "NotFoundError";

const describe = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** The directory at `segments` below `dir`; `null` if there is none (`create` makes it). */
async function descend(dir: Dir, segments: string[], create: boolean): Promise<Dir | null> {
  let at = dir;
  for (const name of segments) {
    try {
      at = await at.getDirectoryHandle(name, { create });
    } catch (e) {
      if (!create && isNotFound(e)) return null;
      throw e;
    }
  }
  return at;
}

async function writeFile(root: Dir, base: string[], path: string, data: Uint8Array): Promise<void> {
  const parts = path.split("/");
  const name = parts.pop() as string;
  const dir = (await descend(root, [...base, ...parts], true)) as Dir;
  const file = await dir.getFileHandle(name, { create: true });
  const out = await file.createWritable();
  try {
    await out.write(data);
  } finally {
    await out.close();
  }
}

/** Removes a file or a whole folder; there is nothing to do if it is not there. */
async function removePath(root: Dir, base: string[], path: string): Promise<void> {
  const parts = path.split("/");
  const name = parts.pop() as string;
  const dir = await descend(root, [...base, ...parts], false);
  if (!dir) return;
  try {
    await dir.removeEntry(name, { recursive: true });
  } catch (e) {
    if (!isNotFound(e)) throw e;
  }
}

async function readFile(dir: Dir, name: string): Promise<Uint8Array | null> {
  try {
    const file = await (await dir.getFileHandle(name)).getFile();
    return new Uint8Array(await file.arrayBuffer());
  } catch (e) {
    if (isNotFound(e)) return null;
    throw e;
  }
}

/** Every file below `dir`, with its path relative to it. */
async function readTree(dir: Dir, prefix = ""): Promise<Files> {
  const out: Files = [];
  for await (const entry of dir.values()) {
    if (entry.kind === "directory") out.push(...(await readTree(entry, `${prefix}${entry.name}/`)));
    else out.push([prefix + entry.name, new Uint8Array(await (await entry.getFile()).arrayBuffer())]);
  }
  return out;
}

const vaultBase = (vault: string): string[] => ["notes", "vaults", vault];
const filesBase = (vault: string): string[] => [...vaultBase(vault), "files"];
const PACKAGES = ["notes", "packages"];
const SETTINGS = { base: ["notes"], file: "settings.json" };

/** Whether the vault was imported before (and so OPFS, not `files.json`, is the source). */
export async function isStored(root: Dir, vault: string): Promise<boolean> {
  const dir = await descend(root, vaultBase(vault), false);
  return dir !== null && (await readFile(dir, "ready")) !== null;
}

/** Reads the vault, the packages and the shared settings kept by an earlier run. No vault (`null`): no vault files. */
export async function readStored(root: Dir, vault: string | null): Promise<Stored> {
  const files = vault === null ? null : await descend(root, filesBase(vault), false);
  const packages = await descend(root, PACKAGES, false);
  const notes = await descend(root, SETTINGS.base, false);
  const settings = notes && (await readFile(notes, SETTINGS.file));
  return {
    vault: files ? await readTree(files) : [],
    packages: packages ? await readTree(packages) : [],
    settings: settings ? decoder.decode(settings) : null,
  };
}

/**
 * Writes the first vault (`files.json`) into OPFS. A start interrupted
 * halfway leaves no `ready` file, so the next start imports again from the
 * empty vault folder.
 */
export async function importFiles(root: Dir, vault: string, files: { vault: Files; packages: Files }): Promise<void> {
  await removePath(root, [], vaultBase(vault).join("/"));
  for (const [path, data] of files.vault) await writeFile(root, filesBase(vault), path, data);
  for (const [path, data] of files.packages) await writeFile(root, PACKAGES, path, data);
  await writeFile(root, vaultBase(vault), "ready", new Uint8Array());
}

/**
 * Writes the changes of the core to OPFS, one flush after each request. With no
 * vault (`null`: the placeholder of the picker screen) the vault files are
 * not written, only the shared settings.
 */
export class Persistence {
  /** Vault paths still to write (or remove): a failed one stays for the next flush. */
  private readonly files = new Set<string>();
  private settings = false;

  constructor(
    private readonly root: Dir,
    private readonly vault: string | null,
    private readonly source: Source,
  ) {}

  /** Forgets what the core reports as changed (the start loaded it from OPFS itself). */
  discard(): void {
    this.source.takeChanges();
  }

  /** Writes what changed since the last flush. A failure is logged, not thrown. */
  async flush(): Promise<void> {
    const changes = this.source.takeChanges();
    if (changes.settings) this.settings = true;
    const vault = this.vault;
    if (vault !== null) {
      for (const path of changes.files) this.files.add(path);
      for (const path of [...this.files]) {
        try {
          // The file is read now: a retry writes its latest content.
          const data = this.source.readFile(path);
          if (data) await writeFile(this.root, filesBase(vault), path, data);
          else await removePath(this.root, filesBase(vault), path);
          this.files.delete(path);
        } catch (e) {
          console.error(`OPFS: cannot save ${path}: ${describe(e)}`);
        }
      }
    }
    if (this.settings) {
      try {
        await writeFile(this.root, SETTINGS.base, SETTINGS.file, encoder.encode(this.source.settingsJson()));
        this.settings = false;
      } catch (e) {
        console.error(`OPFS: cannot save the settings: ${describe(e)}`);
      }
    }
  }
}
