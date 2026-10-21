// What the e2e share for starting the servers they test: the repository paths,
// the built binaries (global-setup.ts builds them once, the specs and workers
// start them directly, not with `cargo run`) and the data directories.
import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

export const ROOT = fileURLToPath(new URL("../../", import.meta.url));
/** The built binaries: the target directory that tools/lib/worktree-target.sh gives (check.sh exports it), else ROOT/target. */
export const BIN = join(process.env.CARGO_TARGET_DIR ?? join(ROOT, "target"), "debug");
/** What the e2e start: one set, so cargo unifies the features once and every later build is a no-op. */
export const BUILD = ["build", "-q", "-p", "notes", "-p", "notes-typst", "-p", "notes-hub"];

/** The data of the e2e app servers: `template` (warmed once) and a copy per Playwright worker, `w<parallelIndex>`. */
export const E2E_DATA = join(ROOT, "tests/.data/e2e");
export const TEMPLATE_DATA = join(E2E_DATA, "template");
export const workerData = (parallelIndex: number): string => join(E2E_DATA, `w${parallelIndex}`);

export const buildBinaries = (): void => {
  execFileSync("cargo", BUILD, { cwd: ROOT, stdio: "inherit" });
};

/** `notes-typst serve` on a data directory (the trash inside it, not the system one). `auth` turns sign-in on. */
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
