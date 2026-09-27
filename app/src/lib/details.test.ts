import { expect, it } from "vitest";
import { detailsKeys, openDetails, restoreDetails } from "./details";

const quiz = (summary = "Ответы") => `<details><summary>${summary}</summary><p>…</p></details>`;
const note = (...parts: string[]) => {
  const div = document.createElement("div");
  div.innerHTML = parts.join("");
  return div;
};

it("ключ — раздел, текст summary и номер среди одинаковых", () => {
  const root = note('<h2 id="a">A</h2>', quiz(), quiz(), quiz("Решение"), '<h3 id="b">B</h3>', quiz());
  expect(detailsKeys(root)).toEqual(["a\nОтветы\n0", "a\nОтветы\n1", "a\nРешение\n0", "b\nОтветы\n0"]);
});

it("блок, добавленный выше, не раскрывает чужой", () => {
  const before = note('<h2 id="a">A</h2>', quiz(), '<h2 id="b">B</h2>', quiz());
  before.querySelectorAll("details")[1]!.open = true;
  const open = openDetails(before);
  // В раздел A добавили ещё одни «Ответы» — по номеру раскрылись бы они.
  const after = note('<h2 id="a">A</h2>', quiz(), quiz(), '<h2 id="b">B</h2>', quiz());
  restoreDetails(after, open);
  expect([...after.querySelectorAll("details")].map((d) => d.open)).toEqual([false, false, true]);
});
