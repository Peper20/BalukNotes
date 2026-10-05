// The list of notes and folders of the vault. Claude Code creates notes at
// any moment, so the list refreshes on every check, without reloading the
// page. Titles are shown (from the note file and the folder's
// `_folder.toml`), not file names.

import { api, type FolderListItem, type NoteListItem } from "../api";
import { splitId } from "../ids";
import { buildTree } from "../tree";

class Notes {
  all = $state.raw<NoteListItem[]>([]);
  folders = $state.raw<FolderListItem[]>([]);
  #folderTitles = $derived(new Map(this.folders.map((f) => [f.path, f.title])));
  /** The tree of folders and notes (the sidebar, the folder page). */
  tree = $derived(buildTree(this.all, (path) => this.folderTitle(path), this.folders.map((f) => f.path)));

  byId(id: string | null): NoteListItem | undefined {
    return id == null ? undefined : this.all.find((n) => n.id === id);
  }

  /** Note title; not in the list (not written yet) - the name from the path. */
  title(id: string): string {
    return this.byId(id)?.title ?? splitId(id).name;
  }

  /** Folder title; not in the list - its name. */
  folderTitle(path: string): string {
    return this.#folderTitles.get(path) ?? path.slice(path.lastIndexOf("/") + 1);
  }

  /** Folder path by titles: `Учёба / Матан`; the root - empty. */
  folderLabel(path: string): string {
    if (!path) return "";
    const parts = path.split("/");
    return parts.map((_, i) => this.folderTitle(parts.slice(0, i + 1).join("/"))).join(" / ");
  }

  /** Notes and folders from the server (at start). */
  async load(): Promise<void> {
    const [all, folders] = await Promise.all([api.notes(), api.folders()]);
    this.all = all;
    this.folders = folders;
  }

  async refresh(): Promise<void> {
    try {
      const [all, folders] = await Promise.all([api.notes(), api.folders()]);
      if (JSON.stringify(all) !== JSON.stringify(this.all)) this.all = all;
      if (JSON.stringify(folders) !== JSON.stringify(this.folders)) this.folders = folders;
    } catch {
      // the server is unreachable: we will try next time
    }
  }
}

export const notes = new Notes();
