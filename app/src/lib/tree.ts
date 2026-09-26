// Дерево заметок по папкам: папки — по алфавиту, заметки — в порядке сервера.

import type { NoteListItem } from "./api";

export interface Folder {
  name: string;
  /** Путь от корня: `Сеть/Linux`; у корня — пусто. */
  path: string;
  folders: Folder[];
  notes: NoteListItem[];
}

export function buildTree(notes: NoteListItem[]): Folder {
  const root: Folder = { name: "", path: "", folders: [], notes: [] };
  const index = new Map<string, Folder>([["", root]]);
  const folder = (path: string): Folder => {
    let f = index.get(path);
    if (f) return f;
    const i = path.lastIndexOf("/");
    const parent = folder(i < 0 ? "" : path.slice(0, i));
    f = { name: path.slice(i + 1), path, folders: [], notes: [] };
    parent.folders.push(f);
    index.set(path, f);
    return f;
  };
  for (const note of notes) folder(note.folder).notes.push(note);
  const sort = (f: Folder) => {
    f.folders.sort((a, b) => a.name.localeCompare(b.name, "ru"));
    f.folders.forEach(sort);
  };
  sort(root);
  return root;
}

/** Сколько заметок в папке вместе с вложенными. */
export function countNotes(f: Folder): number {
  return f.notes.length + f.folders.reduce((sum, sub) => sum + countNotes(sub), 0);
}

/** Папки на пути к заметке — чтобы раскрыть их в дереве. */
export function ancestors(id: string): string[] {
  const parts = id.split("/").slice(0, -1);
  return parts.map((_, i) => parts.slice(0, i + 1).join("/"));
}
