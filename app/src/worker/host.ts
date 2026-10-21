// The queue of the core in the worker: starts it, answers requests one at a
// time, turns failures into API error answers. Plain TypeScript (the worker
// file `core.ts` only wires it to `self`), so it is tested with a fake core.

import type { CoreSource, RequestMessage, ResponseMessage } from "./protocol";

/** What the worker needs of `WasmCore` (the wasm-bindgen glue of `notes-wasm`). */
export interface Core {
  request(method: string, uri: string, body?: Uint8Array): Promise<{ status: number; contentType: string; body: Uint8Array }>;
}

/** Starts the core of `source`: loads the module and the vault files. */
export type Loader = (source: CoreSource) => Promise<Core>;

const encoder = new TextEncoder();

const describe = (e: unknown): string => (e instanceof Error ? e.message : String(e));

const errorAnswer = (id: number, status: number, message: string): ResponseMessage => ({
  id,
  status,
  contentType: "application/json",
  body: encoder.encode(JSON.stringify({ error: message })),
});

export class CoreHost {
  private ready: Promise<Core> | null = null;
  /** Requests are answered one by one: a build blocks the worker, and `WasmCore.request` must not overlap. */
  private queue: Promise<void> = Promise.resolve();
  /** Why every request gets 503: the start failed or the module aborted. */
  private broken: string | null = null;

  constructor(
    private readonly load: Loader,
    private readonly reply: (answer: ResponseMessage) => void,
  ) {}

  start(source: CoreSource): void {
    this.ready = this.load(source);
    // Registered before any request awaits `ready`, so `broken` is set by then.
    this.ready.catch((e: unknown) => {
      this.broken = `the core did not start: ${describe(e)}`;
    });
  }

  request(message: RequestMessage): void {
    this.queue = this.queue.then(async () => this.reply(await this.answer(message)));
  }

  private async answer({ id, method, uri, body }: RequestMessage): Promise<ResponseMessage> {
    let core: Core;
    try {
      core = await (this.ready ?? Promise.reject(new Error("the core is not started")));
    } catch (e) {
      return errorAnswer(id, 503, this.broken ?? describe(e));
    }
    if (this.broken) return errorAnswer(id, 503, this.broken);
    try {
      const r = await core.request(method, uri, body);
      return { id, status: r.status, contentType: r.contentType, body: r.body };
    } catch (e) {
      // A Rust panic aborts the module (no unwinding in wasm32): it answers nothing more.
      if (e instanceof WebAssembly.RuntimeError) this.broken = `the core stopped after an error: ${describe(e)}`;
      return errorAnswer(id, 500, describe(e));
    }
  }
}
