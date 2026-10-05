// The note tree by folders: folders, then notes, alphabetically by titles
// (not file names), numbers by value ("2" before "10").

import type { NoteListItem } from "./api";

export interface Folder {
  /** Folder name on disk: the last path segment. */
  name: string;
  /** Title for display (`_folder.toml`), otherwise the name. */
  title: string;
  /** Path from the root: `Сеть/Linux`; empty for the root. */
  path: string;
  folders: Folder[];
  notes: NoteListItem[];
}

const byTitle = (a: { title: string }, b: { title: string }) => a.title.localeCompare(b.title, "ru", { numeric: true });

/** `folders` also lists folders without notes (empty ones). */
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

/** A tree folder by path; none - `undefined`. */
export function findFolder(root: Folder, path: string): Folder | undefined {
  let f: Folder | undefined = root;
  for (const name of path ? path.split("/") : []) f = f?.folders.find((sub) => sub.name === name);
  return f;
}

/** How many notes the folder has, nested ones included. */
export function countNotes(f: Folder): number {
  return f.notes.length + f.folders.reduce((sum, sub) => sum + countNotes(sub), 0);
}

/** Folders on the way to a note, to expand them in the tree. */
export function ancestors(id: string): string[] {
  const parts = id.split("/").slice(0, -1);
  return parts.map((_, i) => parts.slice(0, i + 1).join("/"));
}
