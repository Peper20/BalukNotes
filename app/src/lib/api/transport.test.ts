import { afterEach, expect, it, vi } from "vitest";
import { api, ApiError, configure, signInRequired } from ".";
import { CoreHost, StartRefused, type Core } from "../../worker/host";
import type { RequestMessage, ResponseMessage, StartMessage } from "../../worker/protocol";
import { setVault } from "../vault";
import { resetSignIn } from "./signin";
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
  resetSignIn();
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
  const status = api.syncStatus();
  worker.answer(3, 204, "");
  expect(await status).toBeNull();
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
  for (const call of [() => api.notes(), () => api.settings()]) {
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

/** A request left pending on purpose: the test only looks at what was sent, and the worker is closed after it. */
const fire = (p: Promise<unknown>): void => void p.catch(() => {});

const NO_WORKER = "the worker must not be spawned";
const noWorker = () => {
  throw new Error(NO_WORKER);
};

/** The HTTP side: `fetch` answers by "METHOD address" and records the requests. */
function stubHub(answers: Record<string, () => Response>): string[] {
  const calls: string[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init: RequestInit = {}) => {
      const key = `${init.method ?? "GET"} ${url}`;
      calls.push(key);
      const answer = answers[key];
      if (!answer) throw new Error(`unexpected request: ${key}`);
      return answer();
    }),
  );
  return calls;
}

const reply = (body: unknown, status = 200) => () => new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

it("with a core the sign-in, the session and the sync API go to the server, not to the worker", async () => {
  const calls = stubHub({
    "GET /api/session": reply({ login: "e2e" }),
    "POST /api/login": reply({}),
    "POST /api/logout": reply({}),
    "GET /api/sync/vaults/A/changes?after=3": reply({ seq: 3, entries: [] }),
  });
  const spawn = vi.fn(noWorker);
  setWorkerFactory(spawn);
  configure({ core: CORE });
  const { transport } = await import("./transport");
  expect(await api.session()).toBe("e2e");
  await api.login("e2e", "pw");
  await api.logout();
  await transport("/api/sync/vaults/A/changes?after=3");
  expect(calls).toEqual(["GET /api/session", "POST /api/login", "POST /api/logout", "GET /api/sync/vaults/A/changes?after=3"]);
  expect(spawn).not.toHaveBeenCalled();
});

it("with a core everything else under /api/ is the worker's, the look-alikes of the server paths too", async () => {
  const fetch = vi.fn();
  vi.stubGlobal("fetch", fetch);
  const worker = new ScriptedWorker();
  setWorkerFactory(() => worker);
  configure({ core: CORE });
  const { transport } = await import("./transport");
  const paths = ["/api/settings", "/api/device/sync", "/api/sessions", "/api/syncing", "/api/vaults/v/notes", "/api/vaults/v/x/y"];
  for (const path of paths) fire(transport(path, { method: "PATCH", body: "{}" }));
  expect(worker.requests.map((r) => r.uri)).toEqual(paths);
  expect(fetch).not.toHaveBeenCalled();
});

it("with a core the vault list is the hub's, in the order of the core's list, and a new vault can be made", async () => {
  // By code points: U+FF21 (one UTF-16 unit, 0xFF21) is before U+1F600 (a pair that starts with 0xD83D).
  const calls = stubHub({
    "GET /api/sync/vaults": reply([
      { name: "\u{1F600}", seq: 1 },
      { name: "Учёба", seq: 4 },
      { name: "Ａ", seq: 0 },
      { name: "abc", seq: 2 },
      { name: "ab", seq: 2 },
    ]),
  });
  setWorkerFactory(noWorker);
  configure({ core: CORE });
  expect(await api.vaults()).toEqual({ vaults: ["ab", "abc", "Учёба", "Ａ", "\u{1F600}"], can_create: true });
  expect(calls).toEqual(["GET /api/sync/vaults"]);
});

it("a hub error on the vault list passes through with its status and body; a 401 shows the sign-in", async () => {
  stubHub({ "GET /api/sync/vaults": reply({ error: "sign in" }, 401) });
  setWorkerFactory(noWorker);
  configure({ core: CORE });
  const e = await api.vaults().catch((e: unknown) => e);
  expect(e).toBeInstanceOf(ApiError);
  expect(e).toMatchObject({ status: 401, message: "sign in" });
  expect(signInRequired()).toBe(true);
});

it("creating a vault is a PUT to the hub, then the list; a hub refusal passes through without the list", async () => {
  const calls = stubHub({
    "PUT /api/sync/vaults/%D0%9D%D0%BE%D0%B2%D0%BE%D0%B5": reply({ name: "Новое", seq: 0 }, 201),
    "GET /api/sync/vaults": reply([
      { name: "Учёба", seq: 4 },
      { name: "Новое", seq: 0 },
    ]),
    "PUT /api/sync/vaults/a%2Fb": reply({ error: "bad vault name" }, 400),
  });
  setWorkerFactory(noWorker);
  configure({ core: CORE });
  expect(await api.createVault("Новое")).toEqual({ vaults: ["Новое", "Учёба"], can_create: true });
  expect(calls).toEqual(["PUT /api/sync/vaults/%D0%9D%D0%BE%D0%B2%D0%BE%D0%B5", "GET /api/sync/vaults"]);
  calls.length = 0;
  await expect(api.createVault("a/b")).rejects.toMatchObject({ status: 400, message: "bad vault name" });
  expect(calls).toEqual(["PUT /api/sync/vaults/a%2Fb"]);
});

it("with a core renaming and deleting a vault answer 405 in Russian and send nothing", async () => {
  const fetch = vi.fn();
  vi.stubGlobal("fetch", fetch);
  const spawn = vi.fn(noWorker);
  setWorkerFactory(spawn);
  configure({ core: CORE });
  const message = "В браузере хранилище нельзя переименовать или удалить";
  await expect(api.renameVault("Учёба", "Другое")).rejects.toMatchObject({ status: 405, message });
  await expect(api.deleteVault("Учёба")).rejects.toMatchObject({ status: 405, message });
  expect(fetch).not.toHaveBeenCalled();
  expect(spawn).not.toHaveBeenCalled();
});

it("the worker starts with the vault of the tab; a vault in the config wins; with none it starts on the placeholder", async () => {
  const workers: ScriptedWorker[] = [];
  const factory = () => setWorkerFactory(() => workers[workers.push(new ScriptedWorker()) - 1]!);

  // Not chosen yet (the picker screen): no vault in the start message.
  factory();
  configure({ core: { module: "/m.js" } });
  fire(api.settings());
  expect(workers[0]!.sent[0]).toEqual({ start: { module: "/m.js" } });
  expect("vault" in (workers[0]!.sent[0] as StartMessage).start).toBe(false);

  // The page names the vault: the core of the tab runs it, once.
  factory();
  setVault("Учёба");
  fire(api.notes());
  fire(api.folders());
  expect(workers).toHaveLength(2);
  expect(workers[1]!.sent[0]).toEqual({ start: { module: "/m.js", vault: "Учёба" } });
  expect(workers[1]!.requests.map((r) => r.uri)).toEqual(["/api/vaults/%D0%A3%D1%87%D1%91%D0%B1%D0%B0/notes", "/api/vaults/%D0%A3%D1%87%D1%91%D0%B1%D0%B0/folders"]);

  // A configured vault is the core's, whatever the address says.
  factory();
  configure({ core: { module: "/m.js", vault: "fixed" } });
  fire(api.settings());
  expect(workers[2]!.sent[0]).toEqual({ start: { module: "/m.js", vault: "fixed" } });
});

it("a vault name the core refuses answers every request 400 with the core's message", async () => {
  setWorkerFactory(() => hostedWorker(() => Promise.reject(new StartRefused('a vault name cannot contain "/"'))));
  configure({ core: { module: "/m.js" } });
  setVault("a/b");
  for (const call of [() => api.notes(), () => api.settings()]) {
    const e = await call().catch((e: unknown) => e);
    expect(e).toBeInstanceOf(ApiError);
    expect(e).toMatchObject({ status: 400, message: 'a vault name cannot contain "/"' });
  }
});
