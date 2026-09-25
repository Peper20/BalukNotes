import { describe, expect, it } from "vitest";
import { fuzzy, highlight } from "./fuzzy";

const rank = (q: string, items: string[]) =>
  items
    .map((s) => ({ s, m: fuzzy(q, s) }))
    .filter((x) => x.m)
    .sort((a, b) => b.m!.score - a.m!.score)
    .map((x) => x.s);

describe("нечёткий поиск", () => {
  it("буквы по порядку, регистр и ё не важны", () => {
    expect(fuzzy("ЁЖ", "ежик")).not.toBeNull();
    expect(fuzzy("пркс", "Префиксные суммы")).not.toBeNull();
    expect(fuzzy("скп", "Префиксные суммы")).toBeNull();
  });

  it("подстрока и начала слов — выше", () => {
    expect(rank("сум", ["Классы эквивалентности", "Префиксные суммы", "Сумма ряда"])).toEqual(["Сумма ряда", "Префиксные суммы"]);
    expect(rank("пс", ["Полярные координаты", "Префиксные суммы"])[0]).toBe("Префиксные суммы");
    expect(rank("ssh", ["Сеть/SSH", "Сеть/UFW"])).toEqual(["Сеть/SSH"]);
  });

  it("позиции для подсветки", () => {
    const m = fuzzy("пс", "Префиксные суммы")!;
    expect(highlight("Префиксные суммы", m.positions)).toEqual([
      { text: "П", hit: true },
      { text: "рефиксные ", hit: false },
      { text: "с", hit: true },
      { text: "уммы", hit: false },
    ]);
  });

  it("пустой запрос совпадает со всем", () => {
    expect(fuzzy("  ", "что угодно")).toEqual({ score: 0, positions: [] });
  });
});
