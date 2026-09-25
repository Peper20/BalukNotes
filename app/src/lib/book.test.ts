import { expect, it } from "vitest";
import { findInBook, showChapter, splitBook } from "./book";

const html = `<style>.x{}</style><article class="k-doc" data-doc="книга">
<header class="k-title"><h1>Книга</h1></header>
<h2 class="k-h k-h1" data-num="1" id="гл-1" data-k-anchor="Основы"><span class="k-num">1</span>Основы</h2>
<p>раз</p><h3 id="Итоги" data-k-anchor="Итоги">Итоги</h3>
<h2 class="k-h k-h1" data-num="2" id="Продолжение" data-k-anchor="Продолжение"><span class="k-num">2</span>Продолжение</h2>
<h3 id="особый" data-k-anchor="Особый-раздел">Особый раздел</h3><h3 id="Итоги-2" data-k-anchor="Итоги">Итоги</h3>
</article>`;

it("делит книгу на главы; титул — в первой", () => {
  const book = splitBook(html)!;
  expect(book.chapters.map((c) => [c.num, c.title])).toEqual([
    ["1", "Основы"],
    ["2", "Продолжение"],
  ]);
  document.body.replaceChildren(book.fragment);
  showChapter(book, 0);
  expect(document.querySelector("h1")?.textContent).toBe("Книга");
  expect(document.getElementById("особый")).toBeNull();
  showChapter(book, 1);
  expect(document.querySelector("h1")).toBeNull();
  expect(document.getElementById("особый")).not.toBeNull();
  expect(document.querySelector("style")).not.toBeNull();
});

it("якорь → глава: по id и по слагу текста", () => {
  const book = splitBook(html)!;
  expect(book.byAnchor.get("особый")).toBe(1);
  expect(book.byAnchor.get("Особый-раздел")).toBe(1);
  expect(book.byAnchor.get("гл-1")).toBe(0);
  expect(findInBook(book, "Итоги-2")?.textContent).toBe("Итоги");
});

it("не книга или одна глава — null", () => {
  expect(splitBook(`<article class="k-doc" data-doc="заметка"><h2 class="k-h1">x</h2></article>`)).toBeNull();
  expect(splitBook(`<article class="k-doc" data-doc="книга"><h2 class="k-h k-h1">x</h2></article>`)).toBeNull();
});
