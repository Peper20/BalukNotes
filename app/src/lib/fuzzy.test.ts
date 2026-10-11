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

  it("a multi-word query that is a substring of the title matches", () => {
    const title = "Поиск в этой заметке или книге";
    const m = fuzzy("в этой заметке или книге", title)!;
    expect(m).not.toBeNull();
    expect(m.positions).toHaveLength("в этой заметке или книге".replace(/ /g, "").length);
    expect(highlight(title, m.positions).map((p) => p.text.trim())).toEqual(["Поиск", "в этой заметке или книге"]);
    expect(rank("этой заметке", ["Заметки этой книги", title, "Другая"])[0]).toBe(title);
  });

  it("a word-start jump does not hide a match", () => {
    expect(fuzzy("aa", "xaa a")).not.toBeNull();
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
