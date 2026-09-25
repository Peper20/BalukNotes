import { describe, expect, it } from "vitest";
import { encodeId, hashAnchor, noteHref, parseRoute, splitId } from "./ids";

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

  it("якорь и имя", () => {
    expect(hashAnchor("#%D0%98%D1%82%D0%BE%D0%B3%D0%B8-2")).toBe("Итоги-2");
    expect(hashAnchor("#")).toBeNull();
    expect(splitId("Сеть/SSH")).toEqual({ name: "SSH", folder: "Сеть" });
    expect(splitId("Начало")).toEqual({ name: "Начало", folder: "" });
  });
});
