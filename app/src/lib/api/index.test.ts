import { afterEach, expect, it, vi } from "vitest";
import { api, ApiError, configure } from ".";
import { setVault } from "../vault";

afterEach(() => {
  configure({ base: "", token: null });
  setVault(null);
  vi.unstubAllGlobals();
});

it("запрос: адрес и токен из настройки", async () => {
  const fetch = vi.fn(async () => new Response("[]", { status: 200 }));
  vi.stubGlobal("fetch", fetch);
  configure({ base: "http://127.0.0.1:9000", token: "t" });
  setVault("a b");
  await api.notes();
  await api.saveSettings({ "appearance.font_size": 20 });
  const calls = fetch.mock.calls as unknown as [string, RequestInit][];
  expect(calls[0]![0]).toBe("http://127.0.0.1:9000/api/vaults/a%20b/notes");
  expect(calls[0]![1].headers).toEqual({ Authorization: "Bearer t" });
  expect(calls[1]![1].headers).toEqual({ Authorization: "Bearer t", "Content-Type": "application/json" });
  expect(api.pdfUrl("a/b", "night")).toBe("http://127.0.0.1:9000/api/vaults/a%20b/pdf/a/b?theme=night&token=t");
  await api.events(7);
  expect(calls[2]![0]).toBe("http://127.0.0.1:9000/api/vaults/a%20b/events?after=7");
  expect(calls[2]![1].headers).toEqual({ Authorization: "Bearer t" });
  // Общее — без хранилища.
  await api.vaults();
  expect(calls[3]![0]).toBe("http://127.0.0.1:9000/api/vaults");
});

it("ошибки сети и сервера — ApiError; отмена — нет", async () => {
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
