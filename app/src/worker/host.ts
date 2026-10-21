// The queue of the core in the worker: starts it, answers requests one at a
// time, turns failures into API error answers, runs the sync rounds of a vault
// of the hub between requests and holds the waiting vault events. Plain
// TypeScript (the worker file `core.ts` only wires it to `self`), so it is
// tested with a fake core.
//
// Sync rounds (architecture §1): one at start, before any request; then 30 s
// after the last one, and 2 s after a request that changed vault files. A
// round is an item of the queue, so it never overlaps a request or another
// round, and at most one waits in the queue.
//
// Vault events (`GET .../events?after=N`, long polling in the client): the core
// answers at once. With no changes after `N` the host holds the request outside
// the queue, so other requests go on, and asks the core again after the next
// round or request that changed vault files, or after 25 s.

import type { CoreSource, RequestMessage, ResponseMessage } from "./protocol";

/** What the worker needs of `WasmCore` (the wasm-bindgen glue of `notes-wasm`). */
export interface Core {
  request(method: string, uri: string, body?: Uint8Array): Promise<{ status: number; contentType: string; body: Uint8Array }>;
  /** Persists what the request changed; runs after the answer, before the next request. Whether vault files changed. Does not throw. */
  flush?(): Promise<boolean | void>;
  /** One sync round of the vault, persisted (a vault of the hub only); whether vault files changed. Does not throw. */
  sync?(): Promise<boolean>;
}

/** A round this long after the last one. */
export const ROUND_EVERY_MS = 30_000;
/** A round this long after a request that changed vault files. */
export const ROUND_AFTER_CHANGE_MS = 2_000;
/** A held events request is answered after this long anyway (as `notes serve` does). */
export const EVENTS_WAIT_MS = 25_000;

/** `GET /api/vaults/<vault>/events?after=N`: the request that waits for changes. */
const EVENTS = /^\/api\/vaults\/[^/?]+\/events\?(?:[^#]*&)?after=\d+(?:&|$)/;

const decoder = new TextDecoder();

/** Whether the answer to an events request has nothing new while the vault reports changes: it may wait. */
function nothingNew(answer: ResponseMessage): boolean {
  if (answer.status !== 200) return false;
  try {
    const events = JSON.parse(decoder.decode(answer.body)) as { watching?: boolean; changes?: unknown[] };
    return events.watching === true && Array.isArray(events.changes) && events.changes.length === 0;
  } catch {
    return false;
  }
}

/** Starts the core of `source`: loads the module and the vault files. */
export type Loader = (source: CoreSource) => Promise<Core>;

/** The core refuses to start for what the request names (a vault name it does not accept): every request is answered 400 with the message. */
export class StartRefused extends Error {}

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
  /** Why every request gets 503 (400 when the start was refused): the start failed or the module aborted. */
  private broken: string | null = null;
  private brokenStatus = 503;
  /** The started core, once it is. */
  private core: Core | null = null;
  /** The timer of the next round and when it fires. */
  private timer: ReturnType<typeof setTimeout> | null = null;
  private due = 0;
  /** A round waits in the queue. */
  private roundQueued = false;
  /** The events requests that wait for a change, with their timers. */
  private readonly held = new Map<RequestMessage, ReturnType<typeof setTimeout>>();

  constructor(
    private readonly load: Loader,
    private readonly reply: (answer: ResponseMessage) => void,
  ) {}

  start(source: CoreSource): void {
    this.ready = this.load(source);
    // Registered before any request awaits `ready`, so `broken` is set by then.
    this.ready.then(
      (core) => (this.core = core),
      (e: unknown) => {
        if (e instanceof StartRefused) {
          this.broken = e.message;
          this.brokenStatus = 400;
        } else {
          this.broken = `the core did not start: ${describe(e)}`;
        }
      },
    );
    // The first round goes before every request.
    this.queueRound();
  }

  request(message: RequestMessage): void {
    const wait = message.method === "GET" && EVENTS.test(message.uri);
    this.enqueue(() => this.serve(message, wait));
  }

  private enqueue(item: () => Promise<void>): void {
    this.queue = this.queue.then(item);
  }

  /**
   * One request from the queue; whatever goes wrong, the next request is
   * served. An events request with nothing new (`wait`) is held instead of
   * answered.
   */
  private async serve(message: RequestMessage, wait = false): Promise<void> {
    let answer: ResponseMessage;
    try {
      answer = await this.answer(message);
    } catch (e) {
      console.error(`request ${message.id}: ${describe(e)}`);
      answer = errorAnswer(message.id, 500, describe(e));
    }
    if (wait && nothingNew(answer)) {
      this.held.set(
        message,
        setTimeout(() => this.release(message), EVENTS_WAIT_MS),
      );
      return;
    }
    this.send(answer, message.id);
    if (await this.flush()) {
      this.wake();
      this.schedule(ROUND_AFTER_CHANGE_MS);
    }
  }

  /** The held events request goes to the queue again, to be answered whatever the core says. */
  private release(message: RequestMessage): void {
    const timer = this.held.get(message);
    if (timer === undefined) return;
    clearTimeout(timer);
    this.held.delete(message);
    this.enqueue(() => this.serve(message));
  }

  /** Vault files changed: every held events request is answered. */
  private wake(): void {
    for (const message of [...this.held.keys()]) this.release(message);
  }

  /** Puts a round into the queue unless one waits there already. */
  private queueRound(): void {
    if (this.roundQueued) return;
    this.roundQueued = true;
    this.enqueue(() => this.round());
  }

  /** A round in `ms` (of a vault of the hub), unless one is due sooner. */
  private schedule(ms: number): void {
    if (!this.core?.sync || this.broken) return;
    const due = Date.now() + ms;
    if (this.timer !== null && this.due <= due) return;
    if (this.timer !== null) clearTimeout(this.timer);
    this.due = due;
    this.timer = setTimeout(() => {
      this.timer = null;
      this.queueRound();
    }, ms);
  }

  /** One round from the queue; the next one is scheduled. */
  private async round(): Promise<void> {
    this.roundQueued = false;
    let core: Core | null;
    try {
      core = this.ready ? await this.ready : null;
    } catch {
      return;
    }
    if (!core?.sync || this.broken) return;
    try {
      if (await core.sync()) this.wake();
    } catch (e) {
      if (e instanceof WebAssembly.RuntimeError) this.broken = `the core stopped after an error: ${describe(e)}`;
      console.error(`sync: ${describe(e)}`);
    }
    this.schedule(ROUND_EVERY_MS);
  }

  /** Posts the answer; if that fails (a failed transfer), tries to answer with an error instead. */
  private send(answer: ResponseMessage, id: number): void {
    try {
      this.reply(answer);
    } catch (e) {
      console.error(`request ${id}: the answer was not delivered: ${describe(e)}`);
      try {
        this.reply(errorAnswer(id, 500, `the answer was not delivered: ${describe(e)}`));
      } catch (e2) {
        console.error(`request ${id}: ${describe(e2)}`);
      }
    }
  }

  /** Persists what the last request changed; whether vault files changed. */
  private async flush(): Promise<boolean> {
    try {
      const core = this.broken ? null : await this.ready;
      return (await core?.flush?.()) === true;
    } catch (e) {
      console.error(`saving the core files: ${describe(e)}`);
      return false;
    }
  }

  private async answer({ id, method, uri, body }: RequestMessage): Promise<ResponseMessage> {
    let core: Core;
    try {
      core = await (this.ready ?? Promise.reject(new Error("the core is not started")));
    } catch (e) {
      return errorAnswer(id, this.brokenStatus, this.broken ?? describe(e));
    }
    if (this.broken) return errorAnswer(id, this.brokenStatus, this.broken);
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
