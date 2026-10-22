// E4 measurement: loads the module, hands it the vault and the packages,
// builds the notes one after another through the HTTP API
// (`GET /api/vaults/<vault>/notes/<id>`) and reports the times and the size of
// the WASM memory (it never shrinks, so it is the peak so far). Runs in the
// worker (worker.js) or, for CPU throttling, on the page itself.
import init, { WasmCore } from "./pkg/notes_wasm.js";

const decode = (b64) => Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
const utf8 = new TextEncoder();
const VAULT = "vault";
const text = new TextDecoder();
const mb = (bytes) => Math.round(bytes / 1e5) / 10;

export async function measure(notes) {
  const t0 = performance.now();
  const module = await WebAssembly.compileStreaming(fetch("./pkg/notes_wasm_bg.wasm"));
  const t1 = performance.now();
  const wasm = await init({ module_or_path: module });
  const t2 = performance.now();
  const files = await (await fetch("./files.json")).json();
  const core = new WasmCore(VAULT);
  // The module carries no fonts: fetched the way the worker does (app/src/worker/load.ts).
  const list = await (await fetch("./fonts.json")).json();
  const fonts = await Promise.all(list.fonts.map(async (path) => new Uint8Array(await (await fetch(`./${path}`)).arrayBuffer())));
  for (const font of fonts) core.addFont(font);
  for (const [path, data] of files.vault) core.addFile(path, decode(data));
  for (const [path, data] of files.packages) core.addPackageFile(path, decode(data));
  const t3 = performance.now();
  core.open();
  const t4 = performance.now();
  const memoryAfterOpenMB = mb(wasm.memory.buffer.byteLength);
  const builds = [];
  const pages = {};
  for (const note of notes) {
    const started = performance.now();
    const response = await core.request("GET", `/api/vaults/${VAULT}/notes/${note.split("/").map(encodeURIComponent).join("/")}`);
    if (response.status !== 200) throw new Error(`${note}: ${response.status} ${text.decode(response.body)}`);
    const page = JSON.parse(text.decode(response.body));
    pages[note] = page.rendered?.body ?? "";
    builds.push({
      note,
      ms: Math.round(performance.now() - started),
      // UTF-8 bytes, as natively.
      htmlKB: Math.round(utf8.encode((page.rendered?.body ?? "") + (page.rendered?.styles ?? "")).length / 1e3),
      errors: page.errors.length,
      memoryMB: mb(wasm.memory.buffer.byteLength),
    });
  }
  return {
    compileMs: Math.round(t1 - t0),
    instantiateMs: Math.round(t2 - t1),
    filesMs: Math.round(t3 - t2),
    openMs: Math.round(t4 - t3),
    memoryAfterOpenMB,
    builds,
    pages,
  };
}
