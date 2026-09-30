import { expect, it } from "vitest";
import type { NoteListItem } from "./api";
import { ancestors, buildTree, countNotes } from "./tree";

const note = (id: string, kind: "note" | "book" = "note", title?: string): NoteListItem => {
  const i = id.lastIndexOf("/");
  const name = id.slice(i + 1);
  return { id, kind, name, folder: i < 0 ? "" : id.slice(0, i), title: title ?? name, tags: [], chapters: [] };
};

it("папки по алфавиту, вложенность, заметки в корне", () => {
  const tree = buildTree([note("Сеть/UFW"), note("Глубоко/а/б/Дно"), note("Книга", "book"), note("Алгоритмы/X")]);
  expect(tree.folders.map((f) => f.name)).toEqual(["Алгоритмы", "Глубоко", "Сеть"]);
  expect(tree.notes.map((n) => n.id)).toEqual(["Книга"]);
  const deep = tree.folders[1]!.folders[0]!.folders[0]!;
  expect(deep.path).toBe("Глубоко/а/б");
  expect(deep.notes[0]!.name).toBe("Дно");
  expect(countNotes(tree)).toBe(4);
  expect(countNotes(tree.folders[1]!)).toBe(1);
});

it("путь к заметке — для раскрытия дерева", () => {
  expect(ancestors("Глубоко/а/б/Дно")).toEqual(["Глубоко", "Глубоко/а", "Глубоко/а/б"]);
  expect(ancestors("Начало")).toEqual([]);
});

it("порядок — по названиям, а не по именам файлов; числа — по значению", () => {
  const titles: Record<string, string> = { Сеть: "Сети и протоколы", Алгоритмы: "Я — последняя" };
  const tree = buildTree(
    [note("Сеть/a", "note", "SSH: основы"), note("Сеть/b", "note", "Шифрование"), note("Алгоритмы/x"), note("Базы/y"), note("г10", "note", "Глава 10"), note("г2", "note", "Глава 2")],
    (path) => titles[path] ?? path,
  );
  expect(tree.folders.map((f) => f.title)).toEqual(["Базы", "Сети и протоколы", "Я — последняя"]);
  expect(tree.folders[1]!.name).toBe("Сеть");
  expect(tree.folders[1]!.notes.map((n) => n.title)).toEqual(["Шифрование", "SSH: основы"]); // русский порядок: кириллица раньше латиницы
  expect(tree.notes.map((n) => n.title)).toEqual(["Глава 2", "Глава 10"]);
});

it("пустые папки — тоже в дереве, со счётчиком 0", () => {
  const tree = buildTree([note("Сеть/UFW")], undefined, ["Пустая", "Сеть/Черновики", "Сеть"]);
  expect(tree.folders.map((f) => f.name)).toEqual(["Пустая", "Сеть"]);
  expect(countNotes(tree.folders[0]!)).toBe(0);
  expect(tree.folders[1]!.folders.map((f) => f.path)).toEqual(["Сеть/Черновики"]);
});
