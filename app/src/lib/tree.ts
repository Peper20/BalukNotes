// Дерево заметок по папкам: папки, затем заметки — по алфавиту названий
// (не имён файлов), числа — по значению («2» раньше «10»).

import type { NoteListItem } from "./api";

export interface Folder {
  /** Имя папки на диске — последний сегмент пути. */
  name: string;
  /** Название для показа (`_folder.toml`), иначе — имя. */
  title: string;
  /** Путь от корня: `Сеть/Linux`; у корня — пусто. */
  path: string;
  folders: Folder[];
  notes: NoteListItem[];
}

const byTitle = (a: { title: string }, b: { title: string }) => a.title.localeCompare(b.title, "ru", { numeric: true });

/** `folders` — ещё и папки без заметок (пустые). */
export function buildTree(
  notes: NoteListItem[],
  folderTitle: (path: string) => string = (p) => p.slice(p.lastIndexOf("/") + 1),
  folders: string[] = [],
): Folder {
  const root: Folder = { name: "", title: "", path: "", folders: [], notes: [] };
  const index = new Map<string, Folder>([["", root]]);
  const folder = (path: string): Folder => {
    let f = index.get(path);
    if (f) return f;
    const i = path.lastIndexOf("/");
    const parent = folder(i < 0 ? "" : path.slice(0, i));
    f = { name: path.slice(i + 1), title: folderTitle(path), path, folders: [], notes: [] };
    parent.folders.push(f);
    index.set(path, f);
    return f;
  };
  for (const path of folders) folder(path);
  for (const note of notes) folder(note.folder).notes.push(note);
  const sort = (f: Folder) => {
    f.folders.sort(byTitle);
    f.notes.sort(byTitle);
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
