// Vault tags: a note has its own, a book those of its root (`main.typ`, all
// chapters inherit them) and of its chapters (`chapter.with(tags: ...)`).

import type { NoteListItem, TaggedChapter } from "./api";

/** Where a tag is: a whole note (book) or a book chapter with its own tag. */
export interface TagPlace {
  note: NoteListItem;
  chapter: TaggedChapter | null;
}

/** A tag and where it is; `notes` is how many notes and books have it (a book counts once, however many chapters carry it). */
export interface TagEntry {
  tag: string;
  places: TagPlace[];
  notes: number;
}

/** All tags of a note: its own (of a book - of the root) and of the chapters, without repeats. */
export const noteTags = (n: NoteListItem): string[] => [...new Set([...n.tags, ...n.chapters.flatMap((c) => c.tags)])];

/**
 * Vault tags: more notes first, then alphabetically. All chapters inherit
 * a tag of the book root - the book as one line; otherwise the chapters with the tag.
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
