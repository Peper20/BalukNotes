// The worker that runs the core: the `notes-wasm` module serves the API
// requests the client sends as messages (`lib/api/transport.ts`). Queue and
// failures - `host.ts`, the start - `load.ts`.

import { CoreHost } from "./host";
import { load, type Env, type Glue, type Seed } from "./load";
import type { Dir } from "./opfs";
import type { ResponseMessage, WorkerMessage } from "./protocol";

/** The worker global scope; the DOM types of the project do not mix with `lib.webworker`. */
interface Scope {
  onmessage: ((e: MessageEvent<WorkerMessage>) => void) | null;
  postMessage(message: unknown, transfer: Transferable[]): void;
}
const scope = self as unknown as Scope;

const env: Env = {
  importGlue: async (address) => (await import(/* @vite-ignore */ address)) as Glue,
  async opfsRoot() {
    try {
      return (await navigator.storage.getDirectory()) as unknown as Dir;
    } catch (e) {
      console.error(`OPFS is not available, nothing is kept between loads: ${e instanceof Error ? e.message : e}`);
      return null;
    }
  },
  async fetchSeed(address) {
    const res = await fetch(address);
    if (!res.ok) throw new Error(`${address}: ${res.status} ${res.statusText}`);
    return (await res.json()) as Seed;
  },
};

const host = new CoreHost(
  (source) => load(source, env),
  (answer: ResponseMessage) => scope.postMessage(answer, [answer.body.buffer]),
);

scope.onmessage = ({ data }) => {
  if ("start" in data) host.start(data.start);
  else host.request(data);
};
