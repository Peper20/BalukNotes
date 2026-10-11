import { afterEach, expect, it } from "vitest";
import { apiConfig, apiUrl, configure, rebaseStylesheets } from "./config";

afterEach(() => configure({ base: "" }));

it("by default the same server", () => {
  expect(apiConfig()).toEqual({ base: "" });
  expect(apiUrl("/api/notes")).toBe("/api/notes");
});

it("the base address is the whole setting, and a token never goes into an address", () => {
  configure({ base: "http://127.0.0.1:9000/" });
  expect(apiUrl("/api/notes")).toBe("http://127.0.0.1:9000/api/notes");
  expect(apiUrl("/api/pdf/a?theme=night")).toBe("http://127.0.0.1:9000/api/pdf/a?theme=night");
  configure({ base: "http://x.test", token: "t" } as Partial<ReturnType<typeof apiConfig>>);
  expect(apiConfig()).toEqual({ base: "http://x.test" });
  expect(apiUrl("/api/events")).toBe("http://x.test/api/events");
});

it("styles come from the configured server", () => {
  // Styles do not load in a <template>.
  const tpl = document.createElement("template");
  tpl.innerHTML = '<link rel="stylesheet" href="/api/fonts.css"><link rel="stylesheet" href="https://x.test/a.css">';
  const doc = tpl.content;
  rebaseStylesheets(doc);
  expect(doc.querySelector("link")!.getAttribute("href")).toBe("/api/fonts.css");
  configure({ base: "http://127.0.0.1:9000" });
  rebaseStylesheets(doc);
  const [a, b] = doc.querySelectorAll("link");
  expect(a!.href).toBe("http://127.0.0.1:9000/api/fonts.css");
  expect(b!.href).toBe("https://x.test/a.css");
});
