import { expect, it } from "vitest";
import type { NoteListItem } from "./api";
import { ancestors, buildTree } from "./tree";

const note = (id: string, kind: "note" | "book" = "note"): NoteListItem => {
  const i = id.lastIndexOf("/");
  return { id, kind, name: id.slice(i + 1), folder: i < 0 ? "" : id.slice(0, i), title: null, tags: [] };
};

it("папки по алфавиту, вложенность, заметки в корне", () => {
  const tree = buildTree([note("Сеть/UFW"), note("Глубоко/а/б/Дно"), note("Книга", "book"), note("Алгоритмы/X")]);
  expect(tree.folders.map((f) => f.name)).toEqual(["Алгоритмы", "Глубоко", "Сеть"]);
  expect(tree.notes.map((n) => n.id)).toEqual(["Книга"]);
  const deep = tree.folders[1]!.folders[0]!.folders[0]!;
  expect(deep.path).toBe("Глубоко/а/б");
  expect(deep.notes[0]!.name).toBe("Дно");
});

it("путь к заметке — для раскрытия дерева", () => {
  expect(ancestors("Глубоко/а/б/Дно")).toEqual(["Глубоко", "Глубоко/а", "Глубоко/а/б"]);
  expect(ancestors("Начало")).toEqual([]);
});
