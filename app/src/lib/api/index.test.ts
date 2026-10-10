import { afterEach, expect, it, vi } from "vitest";
import { api, ApiError, configure, onReach, onSignInRequired, signInMessage, signInRequired } from ".";
import { resetSignIn } from "./signin";
import { setVault } from "../vault";

afterEach(() => {
  configure({ base: "" });
  resetSignIn();
  setVault(null);
  vi.unstubAllGlobals();
});

it("a request: the configured address, the cookie goes with it, no token anywhere", async () => {
  const fetch = vi.fn(async () => new Response("[]", { status: 200 }));
  vi.stubGlobal("fetch", fetch);
  configure({ base: "http://127.0.0.1:9000" });
  setVault("a b");
  await api.notes();
  await api.saveSettings({ "appearance.font_size": 20 });
  const calls = fetch.mock.calls as unknown as [string, RequestInit][];
  expect(calls[0]![0]).toBe("http://127.0.0.1:9000/api/vaults/a%20b/notes");
  expect(calls[0]![1].credentials).toBe("include");
  expect(calls[0]![1].headers).toBeUndefined();
  expect(calls[1]![1].headers).toEqual({ "Content-Type": "application/json" });
  expect(api.pdfUrl("a/b", "night")).toBe("http://127.0.0.1:9000/api/vaults/a%20b/pdf/a/b?theme=night");
  await api.events(7);
  expect(calls[2]![0]).toBe("http://127.0.0.1:9000/api/vaults/a%20b/events?after=7");
  // Shared ones: without a vault.
  await api.vaults();
  expect(calls[3]![0]).toBe("http://127.0.0.1:9000/api/vaults");
});

it("the session: a login, none on a server without sign-in, 401 is the sign-in screen", async () => {
  const answer = (res: Response) => vi.stubGlobal("fetch", async () => res);
  answer(new Response('{"login":"ann"}', { status: 200 }));
  expect(await api.session()).toBe("ann");
  answer(new Response(null, { status: 204 }));
  expect(await api.session()).toBeNull();
  answer(new Response(null, { status: 404 }));
  expect(await api.session()).toBeNull();
  expect(signInRequired()).toBe(false);

  const shown = vi.fn();
  onSignInRequired(shown);
  answer(new Response('{"error":"sign-in required","errors":[]}', { status: 401 }));
  await expect(api.session()).rejects.toMatchObject({ status: 401 });
  expect(shown).toHaveBeenCalledTimes(1);
  expect(signInRequired()).toBe(true);
  // Once is enough, however many requests are refused.
  await expect(api.vaults()).rejects.toBeInstanceOf(ApiError);
  expect(shown).toHaveBeenCalledTimes(1);
});

it("a wrong password is not the sign-in screen, a flood has a wait", async () => {
  const shown = vi.fn();
  onSignInRequired(shown);
  vi.stubGlobal("fetch", async () => new Response('{"error":"wrong login or password","errors":[]}', { status: 401 }));
  const wrong = await api.login("ann", "x").catch((e: unknown) => e);
  expect(wrong).toMatchObject({ status: 401 });
  expect(shown).not.toHaveBeenCalled();
  vi.stubGlobal("fetch", async () => new Response('{"error":"too many attempts: wait 4 s","errors":[]}', { status: 429, headers: { "Retry-After": "4" } }));
  const flood = await api.login("ann", "x").catch((e: unknown) => e);
  expect(flood).toMatchObject({ status: 429, retryAfter: 4 });
  expect(signInMessage(flood)).toBe("Слишком много попыток, подождите 4 с");
  expect(shown).not.toHaveBeenCalled();
});

it("a 401 is an answer: the connection is not lost", async () => {
  const reach = vi.fn();
  onReach(reach);
  vi.stubGlobal("fetch", async () => new Response("{}", { status: 401 }));
  await api.vaults().catch(() => {});
  expect(reach).toHaveBeenLastCalledWith(true);
  onReach(() => {});
});

it("network and server errors are ApiError; a cancel is not", async () => {
  setVault("x");
  vi.stubGlobal("fetch", async () => new Response(JSON.stringify({ error: "нет такой" }), { status: 404 }));
  const notFound = await api.version("x").catch((e: unknown) => e);
  expect(notFound).toBeInstanceOf(ApiError);
  expect(notFound).toMatchObject({ status: 404, message: "нет такой", offline: false });

  vi.stubGlobal("fetch", async () => {
    throw new TypeError("Failed to fetch");
  });
  const offline = await api.version("x").catch((e: unknown) => e);
  expect(offline).toMatchObject({ status: 0, offline: true });

  vi.stubGlobal("fetch", async () => {
    throw new DOMException("aborted", "AbortError");
  });
  const aborted = await api.version("x").catch((e: unknown) => e);
  expect(aborted).not.toBeInstanceOf(ApiError);
});
