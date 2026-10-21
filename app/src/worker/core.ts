// The worker that runs the core: the `notes-wasm` module serves the API
// requests the client sends as messages (`lib/api/transport.ts`). Queue and
// failures - `host.ts`.

import { CoreHost, type Core } from "./host";
import type { CoreSource, ResponseMessage, WorkerMessage } from "./protocol";

/** The worker global scope; the DOM types of the project do not mix with `lib.webworker`. */
interface Scope {
  onmessage: ((e: MessageEvent<WorkerMessage>) => void) | null;
  postMessage(message: unknown, transfer: Transferable[]): void;
}
const scope = self as unknown as Scope;

/** What `notes_wasm.js` exports (wasm-bindgen, `--target web`). */
interface Glue {
  default(): Promise<unknown>;
  WasmCore: new (vault: string) => Core & {
    addFile(path: string, data: Uint8Array): void;
    addPackageFile(path: string, data: Uint8Array): void;
    open(): void;
  };
}

interface Files {
  vault: [string, string][];
  packages: [string, string][];
}

const bytes = (base64: string): Uint8Array => Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));

async function load({ module, vault, files }: CoreSource): Promise<Core> {
  const glue = (await import(/* @vite-ignore */ module)) as Glue;
  await glue.default();
  const core = new glue.WasmCore(vault);
  const res = await fetch(files);
  if (!res.ok) throw new Error(`${files}: ${res.status} ${res.statusText}`);
  const all = (await res.json()) as Files;
  for (const [path, data] of all.vault) core.addFile(path, bytes(data));
  for (const [path, data] of all.packages) core.addPackageFile(path, bytes(data));
  core.open();
  return core;
}

const host = new CoreHost(load, (answer: ResponseMessage) => scope.postMessage(answer, [answer.body.buffer]));

scope.onmessage = ({ data }) => {
  if ("start" in data) host.start(data.start);
  else host.request(data);
};
