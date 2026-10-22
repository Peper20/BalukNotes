// How a request reaches the core: over HTTP to the server, or as a message to
// the core running in a Web Worker (`worker/core.ts`). Both answer with a
// standard `Response`, so `index.ts` handles errors the same way.
//
// With `ApiConfig.core` set, the `/api/...` requests go to the worker, except
// those of the page's server, the hub (`route`); stylesheets, fonts and the PDF
// address stay HTTP (`config.ts`). The worker starts on the first request and
// answers one request at a time (a build blocks it anyway); the queue and the
// failures are in `worker/host.ts`. A worker that asks to be replaced (the core
// aborted, its memory grew too much) is terminated and a fresh one gets the
// calls that had no answer (`CoreClient.restart`).

import type { RequestMessage, ResponseMessage, StartMessage, WorkerReply } from "../../worker/protocol";
import { vault } from "../vault";
import { apiConfig, apiUrl, type CoreSource } from "./config";
import type { VaultsResponse } from "./types/VaultsResponse";

export type Transport = (path: string, init: RequestInit) => Promise<Response>;

/** What the client needs of a `Worker` (a fake one in the tests). */
export interface WorkerLike {
  postMessage(message: StartMessage | RequestMessage, transfer?: Transferable[]): void;
  addEventListener(type: "message", listener: (e: { data: WorkerReply }) => void): void;
  addEventListener(type: "error", listener: (e: { message?: string }) => void): void;
  terminate(): void;
}

/** A call without an answer: kept whole, so a fresh worker can be sent the request again. */
type Pending = { request: RequestMessage; resolve: (r: ResponseMessage) => void; reject: (e: Error) => void };

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
      // The body is copied, not transferred: a restart sends the request again.
      this.pending.set(id, {
        request: { id, method, uri, ...(body && { body }) },
        resolve: (r) => (done(), resolve(r)),
        reject: (e) => (done(), reject(e)),
      });
      worker.postMessage({ id, method, uri, ...(body && { body }) });
    });
  }

  /** Stops the worker; the pending calls fail. */
  close(): void {
    const worker = this.worker;
    this.worker = null;
    worker?.terminate();
    this.failPending(new Error("the core worker was closed"));
  }

  private start(): WorkerLike {
    return this.worker ?? this.spawnWorker(false);
  }

  /** Starts a worker; `fresh`: it replaces one that asked for a restart. */
  private spawnWorker(fresh: boolean): WorkerLike {
    const worker = this.spawn();
    let answered = false;
    worker.addEventListener("message", ({ data }) => {
      if ("restart" in data) {
        if (this.worker === worker) this.restart(worker, data.restart, fresh && !answered);
        return;
      }
      answered = true;
      const p = this.pending.get(data.id);
      this.pending.delete(data.id);
      p?.resolve(data);
    });
    // The script did not load or died outside a request: the pending calls fail,
    // the next call starts a new worker.
    worker.addEventListener("error", (e) => {
      worker.terminate();
      if (this.worker === worker) this.worker = null;
      this.failPending(new Error(e.message || "the core worker failed"));
    });
    worker.postMessage({ start: this.source });
    this.worker = worker;
    return worker;
  }

  /**
   * The worker asked to be replaced: it serves nothing more and never ran the
   * calls without an answer, so they go to a fresh worker as they are. A fresh
   * worker that asks before it answered anything is not replaced again (it
   * would loop): the calls fail and the next call starts a worker.
   */
  private restart(worker: WorkerLike, reason: string, loop: boolean): void {
    worker.terminate();
    if (loop) {
      console.error(`the core worker asked for a restart again before answering: ${reason}`);
      this.worker = null;
      this.failPending(new Error(reason));
      return;
    }
    console.warn(`restarting the core worker: ${reason}`);
    const fresh = this.spawnWorker(true);
    for (const { request } of this.pending.values()) fresh.postMessage(request);
  }

  private failPending(error: Error): void {
    for (const p of this.pending.values()) p.reject(error);
    this.pending.clear();
  }
}

/** The worker of the core, a module worker built by Vite. */
const spawnWorker = (): WorkerLike => new Worker(new URL("../../worker/core.ts", import.meta.url), { type: "module" });

let spawn: () => WorkerLike = spawnWorker;
let current: { core: CoreSource; vault: string | null; client: CoreClient } | null = null;

/** Replaces the worker factory (tests); `null` - the real worker. Stops the running core. */
export function setWorkerFactory(factory: (() => WorkerLike) | null): void {
  spawn = factory ?? spawnWorker;
  current?.client.close();
  current = null;
}

/**
 * The core of the tab: it runs the vault the page shows (`lib/vault.ts`) unless
 * `core.vault` names one. With neither (the picker screen) the worker starts on
 * the placeholder vault (`worker/load.ts`). A tab never changes its vault (the
 * client reloads to open another), so the worker is started once; a changed
 * vault replaces it (tests).
 */
function client(core: CoreSource): CoreClient {
  const name = core.vault === undefined ? vault() : null;
  if (current?.core !== core || current.vault !== name) {
    current?.client.close();
    current = { core, vault: name, client: new CoreClient(name === null ? core : { ...core, vault: name }, spawn) };
  }
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

/** What the page's server, the hub, answers when the core runs in the browser: sign-in and sync. */
const HUB = /^\/api\/(login|logout|session|sync)(\/|$)/;

const jsonResponse = (status: number, body: unknown): Response =>
  new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

/** Vault names in the order of the core's own list: by the bytes of UTF-8, that is by code points. */
function byCodePoints(a: string, b: string): number {
  const x = Array.from(a, (c) => c.codePointAt(0)!);
  const y = Array.from(b, (c) => c.codePointAt(0)!);
  for (let i = 0; i < Math.min(x.length, y.length); i++) if (x[i] !== y[i]) return x[i]! - y[i]!;
  return x.length - y.length;
}

const hubFetch = (path: string, init: RequestInit): Promise<Response> => fetch(apiUrl(path), { credentials: "include", ...init });

/** `GET /api/vaults` from the hub's `GET /api/sync/vaults`: the account's vaults, a new one can be made. A hub error passes through. */
async function hubVaults(signal?: AbortSignal | null): Promise<Response> {
  const res = await hubFetch("/api/sync/vaults", { ...(signal && { signal }) });
  if (!res.ok) return res;
  const list = (await res.json()) as { name: string }[];
  const answer: VaultsResponse = { vaults: list.map((v) => v.name).sort(byCodePoints), can_create: true };
  return jsonResponse(200, answer);
}

/** `POST /api/vaults { name }` as `PUT /api/sync/vaults/{name}`, then the list. A hub error passes through. */
async function hubCreate(body: BodyInit | null | undefined, signal?: AbortSignal | null): Promise<Response> {
  let name: unknown;
  try {
    name = typeof body === "string" ? (JSON.parse(body) as { name?: unknown }).name : undefined;
  } catch {
    name = undefined;
  }
  if (typeof name !== "string") return jsonResponse(400, { error: "no vault name" });
  const created = await hubFetch(`/api/sync/vaults/${encodeURIComponent(name)}`, { method: "PUT", ...(signal && { signal }) });
  return created.ok ? hubVaults(signal) : created;
}

/** The answer of a request the page's server takes instead of the worker; `null`: the worker answers it. */
function route(path: string, init: RequestInit): Promise<Response> | null {
  const pathname = path.split("?")[0]!;
  const method = (init.method ?? "GET").toUpperCase();
  if (HUB.test(pathname)) return hubFetch(path, init);
  if (pathname === "/api/vaults") {
    if (method === "GET") return hubVaults(init.signal);
    if (method === "POST") return hubCreate(init.body, init.signal);
  } else if (/^\/api\/vaults\/[^/]+$/.test(pathname) && (method === "PATCH" || method === "DELETE")) {
    // The hub has no such API.
    return Promise.resolve(jsonResponse(405, { error: "В браузере хранилище нельзя переименовать или удалить" }));
  }
  return null;
}

/**
 * One request: with `core` configured `/api/...` goes to the core in the worker,
 * except sign-in, sync and the vault list, which the server of the page answers;
 * otherwise everything goes to the server.
 */
export function transport(path: string, init: RequestInit = {}): Promise<Response> {
  const { core } = apiConfig();
  if (core && path.startsWith("/api/")) return route(path, init) ?? viaWorker(core, path, init);
  return fetch(apiUrl(path), { credentials: "include", ...init });
}
