// In-memory stand-ins for tests: a directory with the subset of
// `FileSystemDirectoryHandle` that `opfs.ts` uses, and a core's change journal.

import type { Dir, FileHandle, Source } from "./opfs";

const enc = new TextEncoder();
const dec = new TextDecoder();

const notFound = () => new DOMException("not found", "NotFoundError");

/** The subset of `FileSystemDirectoryHandle` the persistence uses, in memory. */
export class FakeDir implements Dir {
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
export class FakeCore implements Source {
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

export const files = (list: [string, string][]): [string, Uint8Array][] => list.map(([p, t]) => [p, enc.encode(t)]);
