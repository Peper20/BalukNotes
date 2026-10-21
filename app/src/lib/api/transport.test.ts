import { afterEach, expect, it, vi } from "vitest";
import { api, ApiError, configure } from ".";
import { CoreHost, type Core } from "../../worker/host";
import type { RequestMessage, ResponseMessage, StartMessage } from "../../worker/protocol";
import { setVault } from "../vault";
import { setWorkerFactory, type WorkerLike } from "./transport";

const CORE = { module: "/wasm/notes_wasm.js", vault: "vault", files: "/wasm/files.json" };
const enc = new TextEncoder();
const dec = new TextDecoder();

/** A worker that only records what it is sent; the test answers by hand. */
class ScriptedWorker implements WorkerLike {
  sent: (StartMessage | RequestMessage)[] = [];
  terminated = false;
  private listeners: { message: ((e: { data: ResponseMessage }) => void)[]; error: ((e: { message?: string }) => void)[] } = {
    message: [],
    error: [],
  };

  postMessage(message: StartMessage | RequestMessage): void {
    this.sent.push(message);
  }
  addEventListener(type: "message" | "error", listener: never): void {
    (this.listeners[type] as unknown[]).push(listener);
  }
  terminate(): void {
    this.terminated = true;
  }

  get requests(): RequestMessage[] {
    return this.sent.filter((m): m is RequestMessage => "id" in m);
  }
  answer(id: number, status: number, body: string, contentType = "application/json"): void {
    for (const l of this.listeners.message) l({ data: { id, status, contentType, body: enc.encode(body) } });
  }
  fail(message: string): void {
    for (const l of this.listeners.error) l({ message });
  }
}

/** A worker with the real queue (`CoreHost`) over a fake core. */
function hostedWorker(load: () => Promise<Core>): WorkerLike {
  const listeners: ((e: { data: ResponseMessage }) => void)[] = [];
  const host = new CoreHost(load, (answer) => queueMicrotask(() => listeners.forEach((l) => l({ data: answer }))));
  return {
    postMessage: (m) => ("start" in m ? host.start(m.start) : host.request(m)),
    addEventListener: ((type: string, l: never) => {
      if (type === "message") listeners.push(l);
    }) as WorkerLike["addEventListener"],
    terminate: () => {},
  };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  configure({ base: "", core: undefined });
  setWorkerFactory(null);
  setVault(null);
  vi.unstubAllGlobals();
});

it("without a core every request goes over HTTP, as before", async () => {
  const fetch = vi.fn(async () => new Response("[]", { status: 200 }));
  vi.stubGlobal("fetch", fetch);
  const spawn = vi.fn(() => new ScriptedWorker());
  setWorkerFactory(spawn);
  setVault("v");
  await api.notes();
  expect(fetch).toHaveBeenCalledWith("/api/vaults/v/notes", { credentials: "include" });
  expect(spawn).not.toHaveBeenCalled();
});

it("with a core the api calls are messages: a Response with the status, type and body", async () => {
  const fetch = vi.fn();
  vi.stubGlobal("fetch", fetch);
  const worker = new ScriptedWorker();
  const spawn = vi.fn(() => worker);
  setWorkerFactory(spawn);
  configure({ core: CORE });
  setVault("vault");

  const notes = api.notes();
  // The worker starts with the first request, once; the start message goes first.
  expect(spawn).toHaveBeenCalledTimes(1);
  expect(worker.sent[0]).toEqual({ start: CORE });
  expect(worker.requests).toEqual([{ id: 1, method: "GET", uri: "/api/vaults/vault/notes" }]);
  worker.answer(1, 200, '[{"id":"a"}]');
  expect(await notes).toEqual([{ id: "a" }]);

  const saved = api.saveSettings({ "device.builds": 3 });
  const post = worker.requests[1]!;
  expect(post).toMatchObject({ id: 2, method: "PUT", uri: "/api/settings" });
  expect(dec.decode(post.body)).toBe('{"device.builds":3}');
  worker.answer(2, 200, '{"device.builds":3}');
  await saved;
  expect(spawn).toHaveBeenCalledTimes(1);

  // An empty answer (204) is a Response without a body; an error answer is an ApiError.
  const session = api.session();
  worker.answer(3, 204, "");
  expect(await session).toBeNull();
  const missing = api.version("x");
  worker.answer(4, 404, '{"error":"no such note","errors":[]}');
  await expect(missing).rejects.toMatchObject({ status: 404, message: "no such note" });
  expect(fetch).not.toHaveBeenCalled();
});

it("only /api/ goes to the worker", async () => {
  const fetch = vi.fn(async () => new Response("", { status: 200 }));
  vi.stubGlobal("fetch", fetch);
  const spawn = vi.fn(() => new ScriptedWorker());
  setWorkerFactory(spawn);
  configure({ core: CORE, base: "http://127.0.0.1:9000" });
  const { transport } = await import("./transport");
  await transport("/fonts/a.woff2", {});
  expect(fetch).toHaveBeenCalledWith("http://127.0.0.1:9000/fonts/a.woff2", { credentials: "include" });
  expect(spawn).not.toHaveBeenCalled();
  setVault("v");
  // The PDF address stays an HTTP one.
  expect(api.pdfUrl("a", "night")).toBe("http://127.0.0.1:9000/api/vaults/v/pdf/a?theme=night");
});

it("requests are handled one at a time, in order, and wait for the start", async () => {
  let started!: (c: Core) => void;
  const log: string[] = [];
  let busy = 0;
  let overlap = false;
  const core: Core = {
    async request(_method, uri) {
      overlap ||= ++busy > 1;
      log.push(`start ${uri}`);
      await tick();
      log.push(`end ${uri}`);
      busy--;
      return { status: 200, contentType: "application/json", body: enc.encode(JSON.stringify(uri)) };
    },
  };
  setWorkerFactory(() => hostedWorker(() => new Promise<Core>((r) => (started = r))));
  configure({ core: CORE });
  setVault("v");

  const a = api.notes();
  const b = api.folders();
  const c = api.graph();
  await tick();
  expect(log).toEqual([]); // the core is starting
  started(core);
  expect(await Promise.all([a, b, c])).toEqual(["/api/vaults/v/notes", "/api/vaults/v/folders", "/api/vaults/v/graph"]);
  expect(log).toEqual([
    "start /api/vaults/v/notes",
    "end /api/vaults/v/notes",
    "start /api/vaults/v/folders",
    "end /api/vaults/v/folders",
    "start /api/vaults/v/graph",
    "end /api/vaults/v/graph",
  ]);
  expect(overlap).toBe(false);
});

it("a cancelled call rejects with AbortError at once; its late answer is dropped", async () => {
  const worker = new ScriptedWorker();
  setWorkerFactory(() => worker);
  configure({ core: CORE });
  setVault("v");

  const controller = new AbortController();
  const search = api.search("x", controller.signal);
  controller.abort();
  await expect(search).rejects.toMatchObject({ name: "AbortError" });
  // Already cancelled: nothing is sent.
  await expect(api.search("y", controller.signal)).rejects.toMatchObject({ name: "AbortError" });
  expect(worker.requests).toHaveLength(1);
  // The worker finishes the request anyway; the next call is not confused by it.
  const next = api.notes();
  worker.answer(1, 200, "[]");
  worker.answer(2, 200, '["next"]');
  expect(await next).toEqual(["next"]);
});

it("a failed start answers 503 with the message to every request", async () => {
  setWorkerFactory(() => hostedWorker(() => Promise.reject(new Error("no module"))));
  configure({ core: CORE });
  setVault("v");
  for (const call of [() => api.notes(), () => api.vaults()]) {
    const e = await call().catch((e: unknown) => e);
    expect(e).toBeInstanceOf(ApiError);
    expect(e).toMatchObject({ status: 503, message: "the core did not start: no module" });
  }
});

it("a panic answers that request 500, then 503", async () => {
  const core: Core = {
    async request(_m, uri) {
      if (uri.endsWith("/graph")) throw new WebAssembly.RuntimeError("unreachable");
      return { status: 200, contentType: "application/json", body: enc.encode("[]") };
    },
  };
  setWorkerFactory(() => hostedWorker(async () => core));
  configure({ core: CORE });
  setVault("v");
  expect(await api.notes()).toEqual([]);
  await expect(api.graph()).rejects.toMatchObject({ status: 500, message: "unreachable" });
  await expect(api.notes()).rejects.toMatchObject({ status: 503 });
});

it("a worker that dies fails the pending calls and is started anew by the next one", async () => {
  const workers: ScriptedWorker[] = [];
  setWorkerFactory(() => workers[workers.push(new ScriptedWorker()) - 1]!);
  configure({ core: CORE });
  setVault("v");
  const first = api.notes();
  workers[0]!.fail("script error");
  await expect(first).rejects.toMatchObject({ status: 0, message: "script error" });
  expect(workers[0]!.terminated).toBe(true);
  const second = api.notes();
  expect(workers).toHaveLength(2);
  workers[1]!.answer(2, 200, "[]");
  expect(await second).toEqual([]);
});
