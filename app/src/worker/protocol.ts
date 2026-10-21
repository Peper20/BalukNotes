// Messages between the client (`lib/api/transport.ts`) and the worker that runs
// the core (`worker/core.ts`).

/** Where the core module and the vault files come from (`ApiConfig.core`). */
export interface CoreSource {
  /** Address of the `notes_wasm.js` glue (the module `notes_wasm_bg.wasm` is next to it). */
  module: string;
  /** The vault name in the API paths (`/api/vaults/<vault>/...`). */
  vault: string;
  /**
   * Address of `files.json`: `{ vault: [[path, base64]], packages: [[path, base64]] }`.
   * It seeds a vault that OPFS does not have yet; afterwards OPFS is the source.
   */
  files?: string;
}

/** The first message of a worker: start the core. */
export interface StartMessage {
  start: CoreSource;
}

/** An API request (`WasmCore.request`). */
export interface RequestMessage {
  id: number;
  method: string;
  /** Path and query, percent-encoded. */
  uri: string;
  body?: Uint8Array;
}

/** The answer to the request `id`; `error` bodies are `{ "error": ... }` JSON. */
export interface ResponseMessage {
  id: number;
  status: number;
  contentType: string;
  body: Uint8Array;
}

export type WorkerMessage = StartMessage | RequestMessage;
