import { afterEach, expect, it, vi } from "vitest";
import { api, ApiError, configure } from ".";

afterEach(() => {
  configure({ base: "", token: null });
  vi.unstubAllGlobals();
});

it("запрос: адрес и токен из настройки", async () => {
  const fetch = vi.fn(async () => new Response("[]", { status: 200 }));
  vi.stubGlobal("fetch", fetch);
  configure({ base: "http://127.0.0.1:9000", token: "t" });
  await api.notes();
  await api.saveSettings({ "appearance.font_size": 20 });
  const calls = fetch.mock.calls as unknown as [string, RequestInit][];
  expect(calls[0]![0]).toBe("http://127.0.0.1:9000/api/notes");
  expect(calls[0]![1].headers).toEqual({ Authorization: "Bearer t" });
  expect(calls[1]![1].headers).toEqual({ Authorization: "Bearer t", "Content-Type": "application/json" });
  expect(api.pdfUrl("a/b", "night")).toBe("http://127.0.0.1:9000/api/pdf/a/b?theme=night&token=t");
});

it("ошибки сети и сервера — ApiError; отмена — нет", async () => {
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
