import { describe, expect, it } from "vitest";
import type { NoteListItem } from "./api";
import { noteTags, tagIndex } from "./tags";

const note = (id: string, tags: string[], chapters: NoteListItem["chapters"] = []): NoteListItem => ({
  id,
  kind: chapters.length ? "book" : "note",
  name: id,
  folder: "",
  title: id,
  tags,
  chapters,
});

describe("теги", () => {
  const book = note("Матан", ["матан"], [
    { title: "Пределы", anchor: "Пределы", tags: ["пределы", "матан"] },
    { title: "Ряды", anchor: "гл-ряды", tags: ["ряды", "пределы"] },
  ]);
  const notes = [note("Предел", ["пределы"]), book, note("SSH", [])];

  it("у книги - теги корня и глав без повторов", () => {
    expect(noteTags(book)).toEqual(["матан", "пределы", "ряды"]);
  });

  it("тег корня - книга одной строкой, тег глав - главы; книга считается один раз", () => {
    const index = tagIndex(notes);
    expect(index.map((e) => [e.tag, e.notes])).toEqual([["пределы", 2], ["матан", 1], ["ряды", 1]]);
    const places = (tag: string) => index.find((e) => e.tag === tag)!.places.map((p) => [p.note.id, p.chapter?.anchor ?? null]);
    expect(places("пределы")).toEqual([["Предел", null], ["Матан", "Пределы"], ["Матан", "гл-ряды"]]);
    expect(places("матан")).toEqual([["Матан", null]]);
  });
});
