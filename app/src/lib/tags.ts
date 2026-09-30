// Теги хранилища: у заметки - свои, у книги - корня (`main.typ`, их наследуют
// все главы) и глав (`chapter.with(tags: ...)`).

import type { NoteListItem, TaggedChapter } from "./api";

/** Где стоит тег: заметка (книга) целиком или глава книги со своим тегом. */
export interface TagPlace {
  note: NoteListItem;
  chapter: TaggedChapter | null;
}

/** Тег и где он стоит; `notes` - сколько заметок и книг с ним (книга - одна, сколько бы глав его ни несли). */
export interface TagEntry {
  tag: string;
  places: TagPlace[];
  notes: number;
}

/** Все теги заметки: свои (у книги - корня) и глав, без повторов. */
export const noteTags = (n: NoteListItem): string[] => [...new Set([...n.tags, ...n.chapters.flatMap((c) => c.tags)])];

/**
 * Теги хранилища: больше заметок - выше, затем по алфавиту. Тег корня книги
 * наследуют все главы - книга одной строкой; иначе - главы с этим тегом.
 */
export function tagIndex(notes: NoteListItem[]): TagEntry[] {
  const byTag = new Map<string, TagEntry>();
  const entry = (tag: string) => byTag.get(tag) ?? byTag.set(tag, { tag, places: [], notes: 0 }).get(tag)!;
  for (const note of notes) {
    for (const tag of noteTags(note)) {
      const e = entry(tag);
      e.notes += 1;
      if (note.tags.includes(tag)) e.places.push({ note, chapter: null });
      else for (const chapter of note.chapters) if (chapter.tags.includes(tag)) e.places.push({ note, chapter });
    }
  }
  return [...byTag.values()].sort((a, b) => b.notes - a.notes || a.tag.localeCompare(b.tag, "ru"));
}
