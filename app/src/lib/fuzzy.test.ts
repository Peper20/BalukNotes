import { describe, expect, it } from "vitest";
import { fuzzy, highlight } from "./fuzzy";

const rank = (q: string, items: string[]) =>
  items
    .map((s) => ({ s, m: fuzzy(q, s) }))
    .filter((x) => x.m)
    .sort((a, b) => b.m!.score - a.m!.score)
    .map((x) => x.s);

describe("fuzzy search", () => {
  it("letters in order, case and ё do not matter", () => {
    expect(fuzzy("ЁЖ", "ежик")).not.toBeNull();
    expect(fuzzy("пркс", "Префиксные суммы")).not.toBeNull();
    expect(fuzzy("скп", "Префиксные суммы")).toBeNull();
  });

  it("a substring and word starts rank higher", () => {
    expect(rank("сум", ["Классы эквивалентности", "Префиксные суммы", "Сумма ряда"])).toEqual(["Сумма ряда", "Префиксные суммы"]);
    expect(rank("пс", ["Полярные координаты", "Префиксные суммы"])[0]).toBe("Префиксные суммы");
    expect(rank("ssh", ["Сеть/SSH", "Сеть/UFW"])).toEqual(["Сеть/SSH"]);
  });

  it("positions for highlighting", () => {
    const m = fuzzy("пс", "Префиксные суммы")!;
    expect(highlight("Префиксные суммы", m.positions)).toEqual([
      { text: "П", hit: true },
      { text: "рефиксные ", hit: false },
      { text: "с", hit: true },
      { text: "уммы", hit: false },
    ]);
  });

  it("an empty query matches everything", () => {
    expect(fuzzy("  ", "что угодно")).toEqual({ score: 0, positions: [] });
  });
});
