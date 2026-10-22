import { afterEach, expect, it, vi } from "vitest";
import { CoreHost, EVENTS_WAIT_MS, MEMORY_LIMIT, ROUND_AFTER_CHANGE_MS, ROUND_EVERY_MS, StartRefused, type Core } from "./host";
import type { ResponseMessage } from "./protocol";

const dec = new TextDecoder();
const tick = () => new Promise((r) => setTimeout(r, 0));

const core = (flush?: Core["flush"]): Core => ({
  request: async (_method, uri) => ({ status: 200, contentType: "text/plain", body: new TextEncoder().encode(uri) }),
  ...(flush && { flush }),
});

/** A host whose reply throws for its first `failures` calls. */
function hostWith(failures: number, flush?: Core["flush"]): { host: CoreHost; answers: ResponseMessage[] } {
  const answers: ResponseMessage[] = [];
  let left = failures;
  const host = new CoreHost(
    async () => core(flush),
    (answer) => {
      if (left-- > 0) throw new Error("DataCloneError");
      answers.push(answer);
    },
    () => {},
  );
  host.start({ module: "m", vault: "v" });
  return { host, answers };
}

afterEach(() => vi.restoreAllMocks());

it("answers the request with an error when its reply throws once, and goes on", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const { host, answers } = hostWith(1);
  host.request({ id: 1, method: "GET", uri: "/one" });
  host.request({ id: 2, method: "GET", uri: "/two" });
  await tick();
  await tick();
  expect(answers.map((a) => [a.id, a.status])).toEqual([
    [1, 500],
    [2, 200],
  ]);
  expect(dec.decode(answers[0]!.body)).toContain("DataCloneError");
  expect(dec.decode(answers[1]!.body)).toBe("/two");
});

it("goes on when even the error answer cannot be delivered", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const { host, answers } = hostWith(2);
  host.request({ id: 1, method: "GET", uri: "/one" });
  host.request({ id: 2, method: "GET", uri: "/two" });
  await tick();
  await tick();
  expect(answers.map((a) => [a.id, a.status])).toEqual([[2, 200]]);
});

it("flushes after each answer, before the next request, and survives a failing flush", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const log: string[] = [];
  let calls = 0;
  const host = new CoreHost(
    async () =>
      core(async () => {
        log.push("flush");
        if (++calls === 1) throw new Error("disk");
      }),
    (answer) => log.push(`answer ${answer.id}`),
    () => {},
  );
  host.start({ module: "m", vault: "v" });
  host.request({ id: 1, method: "GET", uri: "/one" });
  host.request({ id: 2, method: "GET", uri: "/two" });
  await tick();
  await tick();
  expect(log).toEqual(["answer 1", "flush", "answer 2", "flush"]);
});

/** A core of a vault of the hub: events by `seq`, a request to `/change` changes vault files, rounds are logged. */
function hubCore() {
  const state = { seq: 0, log: [] as string[], roundChanges: false, running: 0, maxRunning: 0, gate: null as Promise<void> | null };
  let changed = false;
  const hub: Core = {
    request: async (_method, uri) => {
      state.log.push(uri);
      if (uri === "/change") {
        changed = true;
        state.seq++;
      }
      const after = Number(new URL(uri, "http://x").searchParams.get("after") ?? -1);
      const body = uri.includes("/events") ? { watching: true, seq: state.seq, changes: state.seq > after ? [{ seq: state.seq, paths: [] }] : [] } : {};
      return { status: 200, contentType: "application/json", body: new TextEncoder().encode(JSON.stringify(body)) };
    },
    flush: async () => {
      const out = changed;
      changed = false;
      return out;
    },
    sync: async () => {
      state.log.push("round");
      state.maxRunning = Math.max(state.maxRunning, ++state.running);
      await state.gate;
      state.running--;
      if (!state.roundChanges) return false;
      state.roundChanges = false;
      state.seq++;
      return true;
    },
  };
  return { hub, state };
}

function startHub() {
  const { hub, state } = hubCore();
  const answers: ResponseMessage[] = [];
  const host = new CoreHost(async () => hub, (answer) => answers.push(answer), () => {});
  host.start({ module: "m", vault: "v", hub: "" });
  const answered = () => answers.map((a) => a.id);
  const events = (a: ResponseMessage) => JSON.parse(dec.decode(a.body)) as { changes: unknown[] };
  const rounds = () => state.log.filter((l) => l === "round").length;
  return { host, state, answers, answered, events, rounds };
}

const advance = (ms: number) => vi.advanceTimersByTimeAsync(ms);

it("runs a round at start before any request, every 30 s, 2 s after a change, never two at once", async () => {
  vi.useFakeTimers();
  try {
    const { host, state, answered, rounds } = startHub();
    host.request({ id: 1, method: "GET", uri: "/a" });
    await advance(0);
    expect(state.log).toEqual(["round", "/a"]);
    expect(answered()).toEqual([1]);

    await advance(ROUND_EVERY_MS - 1);
    expect(rounds()).toBe(1);
    await advance(1);
    expect(rounds()).toBe(2);

    // A request that changed vault files: a round 2 s later, not 30.
    host.request({ id: 2, method: "PUT", uri: "/change" });
    await advance(ROUND_AFTER_CHANGE_MS - 1);
    expect(rounds()).toBe(2);
    await advance(1);
    expect(rounds()).toBe(3);

    // A slow round: the requests and the timers wait, no second round starts.
    let open = () => {};
    state.gate = new Promise((r) => (open = r));
    await advance(ROUND_EVERY_MS);
    host.request({ id: 3, method: "PUT", uri: "/change" });
    await advance(5 * ROUND_EVERY_MS);
    expect(state.running).toBe(1);
    expect(answered()).toEqual([1, 2]);
    state.gate = null;
    open();
    await advance(0);
    expect(answered()).toEqual([1, 2, 3]);
    await advance(ROUND_AFTER_CHANGE_MS);
    expect(state.maxRunning).toBe(1);
    expect(rounds()).toBe(5);
  } finally {
    vi.useRealTimers();
  }
});

it("holds an events request with nothing new until a round or a request changes files, or 25 s; others go on", async () => {
  vi.useFakeTimers();
  try {
    const { host, state, answers, answered, events } = startHub();
    await advance(10_000);
    host.request({ id: 1, method: "GET", uri: "/api/vaults/v/events?after=0" });
    host.request({ id: 2, method: "GET", uri: "/api/vaults/v/notes" });
    await advance(0);
    expect(answered()).toEqual([2]);

    // The next round (20 s later) pulls a change: the held request is answered with it.
    state.roundChanges = true;
    await advance(ROUND_EVERY_MS - 10_000);
    expect(answered()).toEqual([2, 1]);
    expect(events(answers[1]!).changes).toHaveLength(1);

    // Nothing changes: answered empty after 25 s.
    host.request({ id: 3, method: "GET", uri: "/api/vaults/v/events?after=1" });
    await advance(EVENTS_WAIT_MS - 1);
    expect(answered()).toEqual([2, 1]);
    await advance(1);
    expect(answered()).toEqual([2, 1, 3]);
    expect(events(answers[2]!).changes).toEqual([]);

    // A request that changes files answers the held one at once.
    host.request({ id: 4, method: "GET", uri: "/api/vaults/v/events?after=1" });
    host.request({ id: 5, method: "PUT", uri: "/change" });
    await advance(0);
    expect(answered()).toEqual([2, 1, 3, 5, 4]);
    expect(events(answers[4]!).changes).toHaveLength(1);

    // Changes already there: answered at once; no `after`: never held.
    host.request({ id: 6, method: "GET", uri: "/api/vaults/v/events?after=0" });
    host.request({ id: 7, method: "GET", uri: "/api/vaults/v/events" });
    await advance(0);
    expect(answered()).toEqual([2, 1, 3, 5, 4, 6, 7]);
  } finally {
    vi.useRealTimers();
  }
});

it("a core without sync runs no rounds", async () => {
  vi.useFakeTimers();
  try {
    const answers: ResponseMessage[] = [];
    const host = new CoreHost(async () => core(async () => true), (a) => answers.push(a), () => {});
    host.start({ module: "m", vault: "v" });
    host.request({ id: 1, method: "PUT", uri: "/x" });
    await advance(10 * ROUND_EVERY_MS);
    expect(answers.map((a) => a.id)).toEqual([1]);
    expect(vi.getTimerCount()).toBe(0);
  } finally {
    vi.useRealTimers();
  }
});

/** What the host posts, in order: `answer <id> <status>` / `flush` / `restart <reason>`. */
function restartHost(load: () => Promise<Core>) {
  const log: string[] = [];
  const host = new CoreHost(
    load,
    (a) => log.push(`answer ${a.id} ${a.status}`),
    (reason) => log.push(`restart ${reason}`),
  );
  host.start({ module: "m", vault: "v" });
  return { host, log };
}

const ok = { status: 200, contentType: "text/plain", body: new Uint8Array() };

it("a RuntimeError on a request: it is answered 500, then one restart; the requests behind get no answer", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const calls: string[] = [];
  const { host, log } = restartHost(async () => ({
    request: async (_m, uri) => {
      calls.push(uri);
      if (uri === "/boom") throw new WebAssembly.RuntimeError("unreachable");
      return ok;
    },
    flush: async () => {
      calls.push("flush");
      return false;
    },
  }));
  host.request({ id: 1, method: "GET", uri: "/boom" });
  host.request({ id: 2, method: "GET", uri: "/after" });
  await tick();
  await tick();
  host.request({ id: 3, method: "GET", uri: "/later" });
  await tick();
  expect(log).toEqual(["answer 1 500", expect.stringMatching(/^restart the core stopped after an error: unreachable/)]);
  // Nothing ran after the abort: not the flush, not the queued request.
  expect(calls).toEqual(["/boom"]);
});

it("a failed start is not a restart: every request is answered 503, or 400 when refused", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  for (const [error, status] of [
    [new Error("no module"), 503],
    [new StartRefused("bad name"), 400],
  ] as const) {
    const { host, log } = restartHost(async () => {
      throw error;
    });
    host.request({ id: 1, method: "GET", uri: "/a" });
    host.request({ id: 2, method: "GET", uri: "/b" });
    await tick();
    await tick();
    expect(log).toEqual([`answer 1 ${status}`, `answer 2 ${status}`]);
  }
});

it("memory over the limit after a request: the answer, the flush, then the restart; at the limit nothing", async () => {
  const log: string[] = [];
  let bytes = MEMORY_LIMIT;
  const host = new CoreHost(
    async () => ({
      request: async () => ok,
      flush: async () => {
        log.push("flush");
        return false;
      },
      memoryBytes: () => bytes,
    }),
    (a) => log.push(`answer ${a.id}`),
    (reason) => log.push(`restart ${reason}`),
  );
  host.start({ module: "m", vault: "v" });
  host.request({ id: 1, method: "GET", uri: "/a" });
  await tick();
  await tick();
  expect(log).toEqual(["answer 1", "flush"]);

  bytes = MEMORY_LIMIT + 1;
  host.request({ id: 2, method: "GET", uri: "/b" });
  host.request({ id: 3, method: "GET", uri: "/c" });
  await tick();
  await tick();
  await tick();
  expect(log).toEqual(["answer 1", "flush", "answer 2", "flush", expect.stringMatching(/^restart the core's memory is 1024 MiB/)]);
});

it("a running sync round delays the restart until it ends, and the request behind it is not served", async () => {
  vi.useFakeTimers();
  try {
    let open = () => {};
    const gate = new Promise<void>((r) => (open = r));
    const served: string[] = [];
    const log: string[] = [];
    const host = new CoreHost(
      async () => ({
        request: async (_m, uri) => (served.push(uri), ok),
        sync: async () => (await gate, false),
        memoryBytes: () => MEMORY_LIMIT + 1,
      }),
      (a) => log.push(`answer ${a.id}`),
      () => log.push("restart"),
    );
    host.start({ module: "m", vault: "v", hub: "" });
    host.request({ id: 1, method: "GET", uri: "/a" });
    await advance(ROUND_EVERY_MS);
    expect(log).toEqual([]);
    open();
    await advance(0);
    expect(log).toEqual(["restart"]);
    expect(served).toEqual([]);
    await advance(10 * ROUND_EVERY_MS);
    expect(log).toEqual(["restart"]);
  } finally {
    vi.useRealTimers();
  }
});

it("a RuntimeError in a sync round restarts the worker; held events requests are dropped", async () => {
  vi.useFakeTimers();
  vi.spyOn(console, "error").mockImplementation(() => {});
  try {
    const { hub } = hubCore();
    let boom = false;
    const log: string[] = [];
    const host = new CoreHost(
      async () => ({
        ...hub,
        sync: async () => {
          if (boom) throw new WebAssembly.RuntimeError("unreachable");
          return false;
        },
      }),
      (a) => log.push(`answer ${a.id}`),
      () => log.push("restart"),
    );
    host.start({ module: "m", vault: "v", hub: "" });
    await advance(10_000);
    host.request({ id: 1, method: "GET", uri: "/api/vaults/v/events?after=0" });
    await advance(0);
    expect(log).toEqual([]);
    boom = true;
    await advance(ROUND_EVERY_MS - 10_000);
    expect(log).toEqual(["restart"]);
    // The held request is not answered by its timer either.
    await advance(EVENTS_WAIT_MS);
    expect(log).toEqual(["restart"]);
    expect(vi.getTimerCount()).toBe(0);
  } finally {
    vi.useRealTimers();
  }
});
