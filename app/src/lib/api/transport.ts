// How a request reaches the core: over HTTP to the server, or as a message to
// the core running in a Web Worker (`worker/core.ts`). Both answer with a
// standard `Response`, so `index.ts` handles errors the same way.
//
// With `ApiConfig.core` set, every `/api/...` request goes to the worker;
// stylesheets, fonts and the PDF address stay HTTP (`config.ts`). The worker
// starts on the first request and answers one request at a time (a build
// blocks it anyway); the queue and the failures are in `worker/host.ts`.

import type { RequestMessage, ResponseMessage, StartMessage } from "../../worker/protocol";
import { apiConfig, apiUrl, type CoreSource } from "./config";

export type Transport = (path: string, init: RequestInit) => Promise<Response>;

/** What the client needs of a `Worker` (a fake one in the tests). */
export interface WorkerLike {
  postMessage(message: StartMessage | RequestMessage, transfer?: Transferable[]): void;
  addEventListener(type: "message", listener: (e: { data: ResponseMessage }) => void): void;
  addEventListener(type: "error", listener: (e: { message?: string }) => void): void;
  terminate(): void;
}

type Pending = { resolve: (r: ResponseMessage) => void; reject: (e: Error) => void };

const abortError = (): Error => new DOMException("The request was aborted", "AbortError");

/** The worker of one core: starts lazily, matches answers to calls by `id`. */
export class CoreClient {
  private worker: WorkerLike | null = null;
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();

  constructor(
    private readonly source: CoreSource,
    private readonly spawn: () => WorkerLike,
  ) {}

  /**
   * One request. An aborted `signal` rejects at once with `AbortError`; the
   * worker still finishes the request and its answer is dropped.
   */
  call(method: string, uri: string, body: Uint8Array | undefined, signal?: AbortSignal | null): Promise<ResponseMessage> {
    if (signal?.aborted) return Promise.reject(abortError());
    const worker = this.start();
    const id = this.nextId++;
    return new Promise<ResponseMessage>((resolve, reject) => {
      const onAbort = () => {
        this.pending.delete(id);
        reject(abortError());
      };
      signal?.addEventListener("abort", onAbort, { once: true });
      const done = () => signal?.removeEventListener("abort", onAbort);
      this.pending.set(id, {
        resolve: (r) => (done(), resolve(r)),
        reject: (e) => (done(), reject(e)),
      });
      worker.postMessage({ id, method, uri, ...(body && { body }) }, body ? [body.buffer] : []);
    });
  }

  private start(): WorkerLike {
    if (this.worker) return this.worker;
    const worker = this.spawn();
    worker.addEventListener("message", ({ data }) => {
      const p = this.pending.get(data.id);
      this.pending.delete(data.id);
      p?.resolve(data);
    });
    // The script did not load or died outside a request: the pending calls fail,
    // the next call starts a new worker.
    worker.addEventListener("error", (e) => {
      worker.terminate();
      if (this.worker === worker) this.worker = null;
      const error = new Error(e.message || "the core worker failed");
      for (const p of this.pending.values()) p.reject(error);
      this.pending.clear();
    });
    worker.postMessage({ start: this.source });
    this.worker = worker;
    return worker;
  }
}

/** The worker of the core, a module worker built by Vite. */
const spawnWorker = (): WorkerLike => new Worker(new URL("../../worker/core.ts", import.meta.url), { type: "module" });

let spawn: () => WorkerLike = spawnWorker;
let current: { source: CoreSource; client: CoreClient } | null = null;

/** Replaces the worker factory (tests); `null` - the real worker. Forgets the running core. */
export function setWorkerFactory(factory: (() => WorkerLike) | null): void {
  spawn = factory ?? spawnWorker;
  current = null;
}

function client(source: CoreSource): CoreClient {
  if (current?.source !== source) current = { source, client: new CoreClient(source, spawn) };
  return current.client;
}

const encoder = new TextEncoder();

/** The same wire as HTTP: a body of a `null`-body status stays empty. */
const NO_BODY = new Set([101, 204, 205, 304]);

async function viaWorker(core: CoreSource, path: string, init: RequestInit): Promise<Response> {
  const { body } = init;
  if (body != null && typeof body !== "string") throw new TypeError("the core takes a string body");
  const answer = await client(core).call((init.method ?? "GET").toUpperCase(), path, body == null ? undefined : encoder.encode(body), init.signal);
  const empty = answer.body.length === 0 || NO_BODY.has(answer.status);
  return new Response(empty ? null : (answer.body as Uint8Array<ArrayBuffer>), {
    status: answer.status,
    ...(answer.contentType && { headers: { "Content-Type": answer.contentType } }),
  });
}

/** One request: `/api/...` to the core in the worker when `core` is configured, otherwise to the server. */
export function transport(path: string, init: RequestInit = {}): Promise<Response> {
  const { core } = apiConfig();
  if (core && path.startsWith("/api/")) return viaWorker(core, path, init);
  return fetch(apiUrl(path), { credentials: "include", ...init });
}
