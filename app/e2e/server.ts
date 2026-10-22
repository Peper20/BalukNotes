// What the e2e share for starting the servers they test: the repository paths,
// the built binaries (global-setup.ts builds them once, the specs and workers
// start them directly, not with `cargo run`) and the data directories.
import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdirSync, realpathSync, renameSync, rmSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { NoteListItem } from "../src/lib/api/types/NoteListItem";
import { vaultCacheKey } from "./cache-key";

export const ROOT = fileURLToPath(new URL("../../", import.meta.url));
/** The built binaries: the target directory that tools/lib/worktree-target.sh gives (check.sh exports it), else ROOT/target. */
export const BIN = join(process.env.CARGO_TARGET_DIR ?? join(ROOT, "target"), "debug");
/** What the e2e start: one set, so cargo unifies the features once and every later build is a no-op. */
export const BUILD = ["build", "-q", "-p", "notes", "-p", "notes-typst", "-p", "notes-hub"];

/** The data of the e2e app servers: `template` (made once, warmed) and a directory per Playwright worker, `w<parallelIndex>`. */
export const E2E_DATA = join(ROOT, "tests/.data/e2e");
export const TEMPLATE_DATA = join(E2E_DATA, "template");
export const workerData = (parallelIndex: number): string => join(E2E_DATA, `w${parallelIndex}`);

/** Copies a directory keeping the file times (`cp -a`). */
export function copyTree(from: string, to: string): void {
  mkdirSync(to, { recursive: true });
  execFileSync("cp", ["-a", `${from}/.`, to]);
}

export const buildBinaries = (): void => {
  execFileSync("cargo", BUILD, { cwd: ROOT, stdio: "inherit" });
};

/**
 * `notes-typst serve` on a data directory (the trash inside it, not the system one). `auth` turns sign-in on.
 * A child of the calling process: it keeps its priority (`check.sh` runs at nice 2).
 */
export function startApp(data: string, port: number, auth = false): ChildProcess {
  const args = ["--data", data, "--trash", join(data, "trash"), "serve", "--addr", `127.0.0.1:${port}`, ...(auth ? ["--auth"] : [])];
  return spawn(join(BIN, "notes-typst"), args, { stdio: ["ignore", "ignore", "inherit"] });
}

/** Resolves when the process has exited. */
export const exitOf = (child: ChildProcess): Promise<void> =>
  new Promise((resolve) => (child.exitCode !== null || child.signalCode !== null ? resolve() : child.once("exit", () => resolve())));

/**
 * Waits until the address answers (any status when `any`). A server that
 * `exited` meanwhile is an error: the port may be taken by a stale one, which
 * would answer instead.
 */
export async function waitFor(url: string, ms: number, any = false, exited?: Promise<unknown>): Promise<void> {
  let gone = false;
  void exited?.then(() => (gone = true));
  const until = Date.now() + ms;
  for (;;) {
    if (gone) throw new Error(`the server for ${url} exited at start (is the port taken?)`);
    try {
      if ((await fetch(url)).ok || any) return;
    } catch {
      // not listening yet
    }
    if (Date.now() > until) throw new Error(`no answer from ${url}`);
    await new Promise((r) => setTimeout(r, 100));
  }
}

/** SIGTERM, and SIGKILL if the process does not exit in 10 s; returns when it is gone (its port is free). */
export async function stop(child: ChildProcess | undefined): Promise<void> {
  if (!child) return;
  const exited = exitOf(child);
  child.kill("SIGTERM");
  const timer = setTimeout(() => child.kill("SIGKILL"), 10_000);
  await exited;
  clearTimeout(timer);
}

/**
 * Makes `data` a copy of the warmed template (global-setup.ts) whose disk cache
 * the server accepts: the file times are kept (`cp -a`), and the cache of the
 * vault, which lies under a hash of the vault's path (e2e/cache-key.ts), is moved
 * under the hash of the copy's path. A template whose cache is not where the hash
 * says is an error: the scheme of the core changed and the copies would be cold.
 */
export function prepareData(data: string, vault: string): void {
  rmSync(data, { recursive: true, force: true });
  copyTree(TEMPLATE_DATA, data);
  const pages = join(data, "cache", "pages");
  const from = vaultCacheKey(realpathSync(join(TEMPLATE_DATA, "vaults", vault)));
  if (!existsSync(join(pages, from)))
    throw new Error(`the template has no disk cache ${from} for its vault: did the cache key of notes-core::cache::DiskCache change? (e2e/cache-key.ts)`);
  renameSync(join(pages, from), join(pages, vaultCacheKey(realpathSync(join(data, "vaults", vault)))));
}

/**
 * Builds every note of the vault on a server, so the waits of the scenarios are
 * short (a book takes seconds in a debug build). Two at a time: the core builds
 * that many notes at once.
 */
export async function warm(base: string, vault: string, headers: Record<string, string> = {}): Promise<number> {
  const api = `${base}/api/vaults/${vault}`;
  const res = await fetch(`${api}/notes`, { headers });
  if (!res.ok) throw new Error(`warming: the note list - ${res.status}`);
  const ids = ((await res.json()) as NoteListItem[]).map((n) => n.id);
  const next = ids[Symbol.iterator]();
  const lane = async () => {
    for (let item = next.next(); !item.done; item = next.next()) {
      const path = item.value.split("/").map(encodeURIComponent).join("/");
      const note = await fetch(`${api}/notes/${path}`, { headers });
      if (!note.ok) throw new Error(`warming: ${item.value} - ${note.status}`);
      await note.arrayBuffer();
    }
  };
  await Promise.all([lane(), lane()]);
  return ids.length;
}
