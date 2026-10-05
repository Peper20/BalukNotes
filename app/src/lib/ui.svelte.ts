// Interface state around a note: book chapters, the outline, panels.

import type { BookView } from "./api";
import type { Find } from "./find";
import { load, save } from "./storage";

/** What is in the tree: a note (book) or a folder (path), for the menu and deleting. */
export type TreeItem = { kind: "note" | "folder"; id: string };

class Ui {
  /** The shown book by chapters (the chapter list and the anchor map from the server) or null. */
  book = $state.raw<BookView | null>(null);
  chapter = $state(0);
  /** id of the heading being read now (highlighted in the outline). */
  currentHeading = $state<string | null>(null);
  /** There is room for the outline to the right of the note column. */
  tocRoom = $state(false);
  tocWidth = $state(240);
  /** The popup outline (a narrow screen or a hidden side one). */
  tocOpen = $state(false);
  /** The sidebar: slides in on a phone, can be hidden on a PC. */
  sidebarOpen = $state(false);
  sidebarHidden = $state(false);
  settingsOpen = $state(false);
  /**
   * The palette: quick open, commands (`>`), search (`/`), tags (`#`);
   * `find` - the Ctrl+F search from a note (where to search - `lib/find.ts`).
   */
  palette = $state<{ query: string; find?: Find | null } | null>(null);
  helpOpen = $state(false);
  /** The "Новое хранилище" dialog. */
  vaultNewOpen = $state(false);
  /** The dialog of the open vault: rename or delete. */
  vaultEdit = $state<"rename" | "delete" | null>(null);
  /** The note or folder asked about deleting (a confirmation dialog). */
  deleting = $state<TreeItem | null>(null);
  /** The note or folder being renamed (a dialog). */
  renaming = $state<TreeItem | null>(null);
  /** The menu of a note or folder in the tree (right click, long tap): where and which. */
  noteMenu = $state<(TreeItem & { x: number; y: number }) | null>(null);
  /** Reading mode: text only, without panels, tabs and the outline. */
  reading = $state(false);

  /** Collapsed tree folders (paths), remembered. */
  collapsed = $state<string[]>(load<string[]>("k-collapsed", []));

  /** A folder was renamed: the collapsed ones move to the new paths. */
  moveCollapsed(moved: (path: string) => string | null): void {
    this.collapsed = this.collapsed.map((p) => moved(p) ?? p);
    save("k-collapsed", this.collapsed);
  }

  setCollapsed(path: string, closed: boolean): void {
    const has = this.collapsed.includes(path);
    if (closed === has) return;
    this.collapsed = closed ? [...this.collapsed, path] : this.collapsed.filter((p) => p !== path);
    save("k-collapsed", this.collapsed);
  }

  openPalette(query = "", find: Find | null = null): void {
    this.palette = { query, find };
  }
}

export const ui = new Ui();

export const mobile = matchMedia("(max-width: 800px)");
