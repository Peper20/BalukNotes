#!/usr/bin/env node
// E4 measurements in headless Chromium (docs/research/E4.md): the prototype
// in a Web Worker builds notes; each scenario runs in a fresh browser.
//
//   crates/notes-wasm/harness/build.sh                 # the module -> tests/.data/wasm/pkg
//   crates/notes-wasm/harness/run.mjs --prepare        # only the vault and packages
//   crates/notes-wasm/harness/run.mjs [--throttle 4] [notes,in,one,worker ...]
//
// `--throttle N` slows the CPU N times (a phone estimate); it applies only to
// the page's main thread, so such a run builds on the page, not in a worker
// (`--page`: on the page without throttling, the baseline).
//
// The vault is a copy of tests/vault plus a generated book of repository
// notes ("Большая книга", BOOK_REPEAT rounds of chapters) in
// tests/.data/wasm/vault; packages (CeTZ and its dependency) are copied from the
// Typst package cache (TYPST_PACKAGES, default ~/.cache/typst/packages) into
// tests/.data/wasm/packages. The native measurement reads the same directories
// (crates/notes-wasm/tests/measure.rs).

import { createServer } from "node:http";
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, extname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { Browser } from "../../../tools/lib/browser.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../../..");
const out = process.env.OUT ?? join(root, "tests/.data/wasm");
const packagesFrom = process.env.TYPST_PACKAGES ?? join(homedir(), ".cache/typst/packages");
// The whitelist (notes-core::packages::ALLOWED).
const PACKAGES = ["preview/cetz/0.4.2", "preview/oxifmt/1.0.0"];
const BOOK = "Большая книга";
const BOOK_REPEAT = Number(process.env.BOOK_REPEAT ?? 2);
// Chapters of the generated book: repository notes without their own template.
const CHAPTERS = ["демо/визуализация", "демо/компоненты", "ассессмент/01-тесты", "Формулы и теги", "Сеть/SSH"];
const DEFAULT_SCENARIOS = [
  "демо/визуализация",
  "демо/компоненты",
  "демо/визуализация,демо/компоненты",
  "Книга",
  BOOK,
];

const files = (dir) =>
  readdirSync(dir, { recursive: true })
    .map((p) => join(dir, p))
    .filter((p) => statSync(p).isFile());

/**
 * `#show: book.with(...)` / `note.with(...)` removed: the note becomes a
 * chapter. In a repeat round its labels get the suffix `-r<round>` (a label
 * occurs once in a document).
 */
function chapter(source, round) {
  if (round > 1) {
    const labels = new Set(
      [...source.matchAll(/<([\p{L}\p{N}_-]+)>|label: "([^"]+)"/gu)].map((m) => m[1] ?? m[2]),
    );
    source = source
      .replace(/([<@])([\p{L}\p{N}_-]+)(>?)/gu, (all, sign, name, close) =>
        labels.has(name) && (sign === "@" || close) ? `${sign}${name}-r${round}${close}` : all,
      )
      .replace(/label: "([^"]+)"/g, (all, name) => `label: "${name}-r${round}"`);
  }
  const start = source.search(/^#show: (book|note)\.with\(/m);
  if (start < 0) return source;
  let depth = 0;
  for (let i = source.indexOf("(", start); i < source.length; i++) {
    if (source[i] === "(") depth++;
    if (source[i] === ")" && --depth === 0) return source.slice(0, start) + source.slice(i + 1);
  }
  return source;
}

function prepare() {
  const vault = join(out, "vault");
  rmSync(vault, { recursive: true, force: true });
  cpSync(join(root, "tests/vault"), vault, { recursive: true });
  const book = join(vault, BOOK);
  mkdirSync(book, { recursive: true });
  const includes = [];
  for (let round = 1; round <= BOOK_REPEAT; round++) {
    for (const [i, note] of CHAPTERS.entries()) {
      const name = `${String(round).padStart(2, "0")}-${i + 1}.typ`;
      writeFileSync(join(book, name), chapter(readFileSync(join(root, "tests/vault", `${note}.typ`), "utf8"), round));
      includes.push(`#include "${name}"`);
    }
  }
  writeFileSync(
    join(book, "main.typ"),
    `#import "/_baluk/lib.typ": *\n#show: book.with(title: [Большая книга], title-page: true, toc: true, depth: 2)\n\n${includes.join("\n")}\n`,
  );
  const packages = join(out, "packages");
  rmSync(packages, { recursive: true, force: true });
  for (const p of PACKAGES) {
    if (!existsSync(join(packagesFrom, p))) {
      throw new Error(`no ${p} in ${packagesFrom}: build a note natively once (notes check) to download it`);
    }
    cpSync(join(packagesFrom, p), join(packages, p), { recursive: true });
  }
  const entries = (dir) => files(dir).map((p) => [relative(dir, p), readFileSync(p).toString("base64")]);
  writeFileSync(join(out, "files.json"), JSON.stringify({ vault: entries(vault), packages: entries(packages) }));
  const size = (dir) => files(dir).reduce((n, p) => n + statSync(p).size, 0);
  console.log(`vault ${size(vault)} B, packages ${size(packages)} B -> ${out}`);
}

const TYPES = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".json": "application/json" };

function serve() {
  const server = createServer((req, res) => {
    const path = decodeURIComponent(new URL(req.url, "http://x").pathname).slice(1);
    const file = ["index.html", "worker.js", "measure.js"].includes(path) ? join(here, path) : join(out, path);
    if (!file.startsWith(out) && !file.startsWith(here)) return res.writeHead(403).end();
    if (!existsSync(file)) return res.writeHead(404).end();
    res.writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream" });
    res.end(readFileSync(file));
  });
  return new Promise((r) => server.listen(0, "127.0.0.1", () => r(server)));
}

/** Peak RSS (VmHWM) of the renderer processes of this browser, MB. */
function rendererPeakMB(browserPid) {
  const parent = (pid) => Number(readFileSync(`/proc/${pid}/stat`, "utf8").split(") ")[1].split(" ")[1]);
  let peak = 0;
  for (const pid of readdirSync("/proc").filter((p) => /^\d+$/.test(p))) {
    try {
      if (!readFileSync(`/proc/${pid}/cmdline`, "utf8").includes("--type=renderer")) continue;
      let p = Number(pid);
      while (p > 1 && p !== browserPid) p = parent(p);
      if (p !== browserPid) continue;
      const hwm = readFileSync(`/proc/${pid}/status`, "utf8").match(/VmHWM:\s+(\d+) kB/);
      if (hwm) peak = Math.max(peak, Number(hwm[1]) / 1024);
    } catch {
      // the process is gone
    }
  }
  return Math.round(peak);
}

async function run(port, notes, throttle) {
  const browser = await Browser.launch();
  try {
    // CDP CPU throttling applies to the page's main thread only (a worker
    // session refuses it): a throttled run builds on the page itself.
    if (throttle > 1) await browser.send("Emulation.setCPUThrottlingRate", { rate: throttle });
    const main = throttle > 1 || onPage ? "&main=1" : "";
    await browser.send("Page.navigate", { url: `http://127.0.0.1:${port}/index.html?notes=${encodeURIComponent(notes)}${main}` });
    await browser.evaluate(`new Promise(r => document.readyState === "complete" ? r() : addEventListener("load", r))`);
    const { pages = {}, ...result } = await browser.evaluate("window.result");
    // The page bodies, to compare with the native build (tests/measure.rs writes `.native.html`).
    mkdirSync(join(out, "html"), { recursive: true });
    for (const [note, body] of Object.entries(pages)) writeFileSync(join(out, "html", `${note.replaceAll("/", "_")}.wasm.html`), body);
    return { notes, throttle, where: main ? "page" : "worker", ...result, rendererPeakMB: rendererPeakMB(browser.proc.pid) };
  } finally {
    await browser.close();
  }
}

const args = process.argv.slice(2);
const flag = (name) => {
  const i = args.indexOf(name);
  return i < 0 ? undefined : args.splice(i, 2)[1];
};
const throttle = Number(flag("--throttle") ?? 1);
// On the page without throttling: the baseline of a throttled run.
const onPage = args.includes("--page") && args.splice(args.indexOf("--page"), 1).length > 0;
if (args[0] === "--prepare") {
  prepare();
  process.exit(0);
}
if (!existsSync(join(out, "pkg/notes_wasm_bg.wasm"))) {
  console.error(`no module in ${out}/pkg: run crates/notes-wasm/harness/build.sh`);
  process.exit(2);
}
prepare();
const server = await serve();
for (const notes of args.length ? args : DEFAULT_SCENARIOS) {
  console.log(JSON.stringify(await run(server.address().port, notes, throttle)));
}
server.close();
