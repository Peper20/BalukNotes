import { afterEach, expect, it, vi } from "vitest";
import { CoreHost, type Core } from "./host";
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
  );
  host.start({ module: "m", vault: "v" });
  host.request({ id: 1, method: "GET", uri: "/one" });
  host.request({ id: 2, method: "GET", uri: "/two" });
  await tick();
  await tick();
  expect(log).toEqual(["answer 1", "flush", "answer 2", "flush"]);
});
