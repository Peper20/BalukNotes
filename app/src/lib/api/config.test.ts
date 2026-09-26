import { afterEach, expect, it } from "vitest";
import { apiConfig, apiUrl, authHeaders, configure, rebaseStylesheets } from "./config";

afterEach(() => configure({ base: "", token: null }));

it("по умолчанию — тот же сервер, без токена", () => {
  expect(apiConfig()).toEqual({ base: "", token: null });
  expect(apiUrl("/api/notes")).toBe("/api/notes");
  expect(apiUrl("/api/pdf/a", { withToken: true })).toBe("/api/pdf/a");
  expect(authHeaders()).toEqual({});
});

it("базовый адрес и токен — одной настройкой", () => {
  configure({ base: "http://127.0.0.1:9000/", token: "s3 cret" });
  expect(apiUrl("/api/notes")).toBe("http://127.0.0.1:9000/api/notes");
  expect(apiUrl("/api/pdf/a?theme=night", { withToken: true })).toBe("http://127.0.0.1:9000/api/pdf/a?theme=night&token=s3%20cret");
  expect(apiUrl("/api/events", { withToken: true })).toBe("http://127.0.0.1:9000/api/events?token=s3%20cret");
  expect(authHeaders()).toEqual({ Authorization: "Bearer s3 cret" });
  configure({ token: "" });
  expect(apiConfig()).toEqual({ base: "http://127.0.0.1:9000", token: null });
});

it("стили — с настроенного сервера", () => {
  // В <template> стили не загружаются.
  const tpl = document.createElement("template");
  tpl.innerHTML = '<link rel="stylesheet" href="/api/fonts.css"><link rel="stylesheet" href="https://x.test/a.css">';
  const doc = tpl.content;
  rebaseStylesheets(doc);
  expect(doc.querySelector("link")!.getAttribute("href")).toBe("/api/fonts.css");
  configure({ base: "http://127.0.0.1:9000", token: "t" });
  rebaseStylesheets(doc);
  const [a, b] = doc.querySelectorAll("link");
  expect(a!.href).toBe("http://127.0.0.1:9000/api/fonts.css?token=t");
  expect(b!.href).toBe("https://x.test/a.css");
});
