// Builds `assets/baluk.css` from the block files (`baluk.css` is their
// order): `@import "./...";` is replaced by the file content, then
// `forBrowsers`. Runs in Node, from the Vite plugin (`plugin.ts`), not in the browser.

import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { transform } from "lightningcss";

/** The file with the block order. */
export const ENTRY = resolve(dirname(fileURLToPath(import.meta.url)), "baluk.css");

const IMPORT = /^@import\s+"([^"]+)";[ \t]*$/gm;

/**
 * Joins the file `file` with its `@import`s (recursively); `seen` are the
 * files read (for watching in dev mode).
 */
export async function bundle(file = ENTRY, seen: Set<string> = new Set()): Promise<string> {
  if (seen.has(file)) throw new Error(`baluk.css: ${file} is included twice`);
  seen.add(file);
  const text = await readFile(file, "utf8");
  const parts = await Promise.all([...text.matchAll(IMPORT)].map((m) => bundle(resolve(dirname(file), m[1]!), seen)));
  let i = 0;
  return text.replace(IMPORT, () => parts[i++]!.trimEnd());
}

/**
 * Browsers the prefixes are added for: the first ones with CSS nesting (the
 * blocks already use it, no need to unfold it). The app window is WebKitGTK:
 * without `-webkit-user-select` the graph got selected entirely when dragging.
 */
const TARGETS = { chrome: 120 << 16, firefox: 117 << 16, safari: (17 << 16) | (2 << 8) };

/** Joined -> CSS for browsers: prefixes (`-webkit-user-select`...), not minified. */
export function forBrowsers(css: string): string {
  const out = transform({ filename: "baluk.css", code: new TextEncoder().encode(css), targets: TARGETS });
  return new TextDecoder().decode(out.code);
}
