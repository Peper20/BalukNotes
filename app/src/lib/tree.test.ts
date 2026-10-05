import { expect, it } from "vitest";
import type { NoteListItem } from "./api";
import { ancestors, buildTree, countNotes, findFolder } from "./tree";

const note = (id: string, kind: "note" | "book" = "note", title?: string): NoteListItem => {
  const i = id.lastIndexOf("/");
  const name = id.slice(i + 1);
  return { id, kind, name, folder: i < 0 ? "" : id.slice(0, i), title: title ?? name, tags: [], chapters: [] };
};

it("folders alphabetically, nesting, notes in the root", () => {
  const tree = buildTree([note("Сеть/UFW"), note("Глубоко/а/б/Дно"), note("Книга", "book"), note("Алгоритмы/X")]);
  expect(tree.folders.map((f) => f.name)).toEqual(["Алгоритмы", "Глубоко", "Сеть"]);
  expect(tree.notes.map((n) => n.id)).toEqual(["Книга"]);
  const deep = tree.folders[1]!.folders[0]!.folders[0]!;
  expect(deep.path).toBe("Глубоко/а/б");
  expect(deep.notes[0]!.name).toBe("Дно");
  expect(countNotes(tree)).toBe(4);
  expect(countNotes(tree.folders[1]!)).toBe(1);
  expect(findFolder(tree, "Глубоко/а/б")).toBe(deep);
  expect(findFolder(tree, "")).toBe(tree);
  expect(findFolder(tree, "Глубоко/нет")).toBeUndefined();
});

it("the path to a note, to expand the tree", () => {
  expect(ancestors("Глубоко/а/б/Дно")).toEqual(["Глубоко", "Глубоко/а", "Глубоко/а/б"]);
  expect(ancestors("Начало")).toEqual([]);
});

it("order by titles, not file names; numbers by value", () => {
  const titles: Record<string, string> = { Сеть: "Сети и протоколы", Алгоритмы: "Я — последняя" };
  const tree = buildTree(
    [note("Сеть/a", "note", "SSH: основы"), note("Сеть/b", "note", "Шифрование"), note("Алгоритмы/x"), note("Базы/y"), note("г10", "note", "Глава 10"), note("г2", "note", "Глава 2")],
    (path) => titles[path] ?? path,
  );
  expect(tree.folders.map((f) => f.title)).toEqual(["Базы", "Сети и протоколы", "Я — последняя"]);
  expect(tree.folders[1]!.name).toBe("Сеть");
  expect(tree.folders[1]!.notes.map((n) => n.title)).toEqual(["Шифрование", "SSH: основы"]); // Russian order: Cyrillic before Latin
  expect(tree.notes.map((n) => n.title)).toEqual(["Глава 2", "Глава 10"]);
});

it("empty folders are in the tree too, with a count of 0", () => {
  const tree = buildTree([note("Сеть/UFW")], undefined, ["Пустая", "Сеть/Черновики", "Сеть"]);
  expect(tree.folders.map((f) => f.name)).toEqual(["Пустая", "Сеть"]);
  expect(countNotes(tree.folders[0]!)).toBe(0);
  expect(tree.folders[1]!.folders.map((f) => f.path)).toEqual(["Сеть/Черновики"]);
});
