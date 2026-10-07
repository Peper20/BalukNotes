#!/usr/bin/env node
// Visual check: the home page and all server notes in the light and dark
// theme and on a narrow screen, in one run. The result is an album index.html
// (open it in a browser and look through).
//
//   tools/test-env.sh &                       # test environment on :8432
//   tools/visual.mjs [--base http://127.0.0.1:8432] [--out tests/.data/visual]
//                    [--only substring] [--full]
//
//   --only   only notes whose id contains the substring
//   --full   the whole page (otherwise the first screen)
//
// The light/dark theme goes via prefers-color-scheme, so the server's theme
// setting must be "as in the system" (as in a fresh tests/.data).

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { Browser } from "./lib/browser.mjs";

const args = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = args.indexOf(name);
  return i < 0 ? fallback : args[i + 1];
};
const base = flag("--base", "http://127.0.0.1:8432").replace(/\/$/, "");
const out = resolve(flag("--out", "tests/.data/visual"));
const only = flag("--only", null);
const full = args.includes("--full");

const VIEWS = [
  { name: "light", width: 1300, height: 900, dark: false },
  { name: "dark", width: 1300, height: 900, dark: true },
  { name: "narrow", width: 400, height: 800, dark: false },
];

const encodeId = (id) => id.split("/").map(encodeURIComponent).join("/");
const fileName = (i, id, view) => `${String(i).padStart(2, "0")}-${id.replace(/[^\p{L}\p{N}]+/gu, "_")}-${view}.png`;
const esc = (s) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

let notes, vault;
try {
  // The server's first vault (tools/test-env.sh has one: tests/vault).
  const name = encodeURIComponent((await (await fetch(`${base}/api/vaults`)).json()).vaults[0]);
  vault = `/v/${name}`;
  notes = await (await fetch(`${base}/api/vaults/${name}/notes`)).json();
} catch (e) {
  console.error(`server ${base} does not respond (${e.cause?.code ?? e.message}). Start tools/test-env.sh`);
  process.exit(1);
}
const pages = [{ id: "", title: "Home", url: `${base}${vault}/` }].concat(
  notes.filter((n) => !only || n.id.includes(only)).map((n) => ({ id: n.id, title: n.id, url: `${base}${vault}/n/${encodeId(n.id)}` })),
);

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const browser = await Browser.launch();
const rows = [];
const started = Date.now();
try {
  for (const [i, page] of pages.entries()) {
    const shots = [];
    for (const view of VIEWS) {
      await browser.viewport(view.width, view.height);
      await browser.dark(view.dark);
      await browser.open(page.url);
      const file = fileName(i, page.id || "home", view.name);
      await browser.screenshot(join(out, file), { full });
      shots.push({ view: view.name, file });
    }
    rows.push({ page, shots });
    process.stdout.write(`\r${i + 1}/${pages.length} ${page.title.padEnd(40)}`);
  }
} finally {
  await browser.close();
}

writeFileSync(join(out, "index.html"), `<!doctype html><meta charset="utf-8"><title>Visual check</title>
<style>
  body { font: 14px system-ui, sans-serif; margin: 16px; background: #eee; }
  h2 { font-size: 15px; margin: 28px 0 8px; }
  .row { display: flex; gap: 12px; align-items: flex-start; }
  figure { margin: 0; background: #fff; padding: 6px; border-radius: 6px; }
  figcaption { color: #666; font-size: 12px; margin-bottom: 4px; }
  img { display: block; max-width: 560px; height: auto; border: 1px solid #ddd; }
  img.narrow { max-width: 200px; }
</style>
<h1>${esc(base)} - ${rows.length} pages, ${new Date().toLocaleString("en-GB")}</h1>
${rows.map(({ page, shots }) => `<h2><a href="${esc(page.url)}">${esc(page.title)}</a></h2><div class="row">${shots
  .map((s) => `<figure><figcaption>${s.view}</figcaption><a href="${encodeURI(s.file)}"><img loading="lazy" class="${s.view === "narrow" ? "narrow" : ""}" src="${encodeURI(s.file)}"></a></figure>`)
  .join("")}</div>`).join("\n")}
`);
console.log(`\nscreenshots: ${rows.length * VIEWS.length} in ${Math.round((Date.now() - started) / 1000)} s -> ${join(out, "index.html")}`);
