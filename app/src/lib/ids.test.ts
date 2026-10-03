import { afterEach, describe, expect, it } from "vitest";
import { encodeId, folderGraphHref, folderHref, graphHref, hashAnchor, homeHref, isAppPath, noteHref, parseRoute, splitId, tagHref } from "./ids";
import { setVault } from "./vault";

describe("адреса заметок", () => {
  it("кодирует сегменты, оставляя /", () => {
    const [names, and] = [encodeURIComponent("Имена"), encodeURIComponent("и")];
    expect(encodeId("Имена/C++ и C#")).toBe(`${names}/C%2B%2B%20${and}%20C%23`);
    expect(noteHref("Сеть/SSH", "Смена порта")).toBe(`/n/${encodeURIComponent("Сеть")}/SSH#${encodeURIComponent("Смена порта")}`);
  });

  it("читает маршрут обратно, в том числе % и #", () => {
    for (const id of ["Сеть/SSH", "Имена/C++ и C#", "Имена/50% готово", "Глубоко/а/б/в/г/Дно"]) {
      expect(parseRoute(new URL(noteHref(id), "http://x").pathname)).toEqual({ kind: "note", id });
    }
    expect(parseRoute("/")).toEqual({ kind: "home" });
    expect(parseRoute("/n/")).toEqual({ kind: "home" });
    // Битое кодирование — главная, а не исключение.
    expect(parseRoute("/n/%E0%A4%A")).toEqual({ kind: "home" });
  });

  it("граф: весь и соседи заметки", () => {
    expect(parseRoute("/graph")).toEqual({ kind: "graph", around: null, depth: 1, folder: null });
    const url = new URL(graphHref("Имена/C++ и C#", 2), "http://x");
    expect(parseRoute(url.pathname, url.search)).toEqual({ kind: "graph", around: "Имена/C++ и C#", depth: 2, folder: null });
    expect(parseRoute("/graph", "?around=A&depth=99")).toEqual({ kind: "graph", around: "A", depth: 1, folder: null });
    expect(graphHref("A")).toBe("/graph?around=A");
    // Папка: страница и граф её поддерева.
    expect(parseRoute(new URL(folderHref("Мат/Анализ: предел"), "http://x").pathname)).toEqual({ kind: "folder", path: "Мат/Анализ: предел" });
    const g = new URL(folderGraphHref("Мат/Анализ"), "http://x");
    expect(parseRoute(g.pathname, g.search)).toEqual({ kind: "graph", around: null, depth: 1, folder: "Мат/Анализ" });
  });

  it("якорь и имя", () => {
    expect(hashAnchor("#%D0%98%D1%82%D0%BE%D0%B3%D0%B8-2")).toBe("Итоги-2");
    expect(hashAnchor("#")).toBeNull();
    expect(splitId("Сеть/SSH")).toEqual({ name: "SSH", folder: "Сеть" });
    expect(splitId("Начало")).toEqual({ name: "Начало", folder: "" });
  });
});

describe("адреса в хранилище", () => {
  afterEach(() => setVault(null));

  it("начинаются с хранилища и читаются обратно", () => {
    setVault("Учёба");
    const base = `/v/${encodeURIComponent("Учёба")}`;
    expect(noteHref("A", "x")).toBe(`${base}/n/A#x`);
    expect(tagHref("сеть")).toBe(`${base}/tags/${encodeURIComponent("сеть")}`);
    expect(graphHref()).toBe(`${base}/graph`);
    expect(graphHref("A")).toBe(`${base}/graph?around=A`);
    expect(homeHref()).toBe(`${base}/`);
    expect(parseRoute(new URL(noteHref("Сеть/SSH"), "http://x").pathname)).toEqual({ kind: "note", id: "Сеть/SSH" });
    expect(parseRoute(`${base}/`)).toEqual({ kind: "home" });
    expect(parseRoute(base)).toEqual({ kind: "home" });
    expect(parseRoute(`${base}/graph`, "?around=A")).toEqual({ kind: "graph", around: "A", depth: 1, folder: null });
    expect(parseRoute("/n/A")).toEqual({ kind: "note", id: "A" });
  });

  it("свои адреса — клиенту, чужого хранилища — нет", () => {
    setVault("Учёба");
    const base = `/v/${encodeURIComponent("Учёба")}`;
    for (const own of [`${base}/`, base, `${base}/n/A`, `${base}/tags`, `${base}/graph`, `${base}/f/A`, "/n/A", "/", "/graph"]) {
      expect(isAppPath(own), own).toBe(true);
    }
    for (const other of ["/v/Другое/n/A", "/v/Другое/", `${base}x/n/A`, "/assets/x.css", "/api/notes"]) {
      expect(isAppPath(other), other).toBe(false);
    }
  });
});
