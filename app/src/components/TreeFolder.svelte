<!-- A folder of the note tree: subfolders, then notes. Recursive. -->
<script lang="ts">
  import BookIcon from "@lucide/svelte/icons/book";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import FileText from "@lucide/svelte/icons/file-text";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import { router } from "../lib/state";
  import { noteHref } from "../lib/ids";
  import { countNotes, type Folder } from "../lib/tree";
  import { ui } from "../lib/ui.svelte";
  import TreeFolder from "./TreeFolder.svelte";

  let { folder }: { folder: Folder } = $props();
  const icon = { size: 16, strokeWidth: 1.75, "aria-hidden": true } as const;

  /** Right click (on a phone - a long tap): the menu of a note or folder. */
  function menu(e: MouseEvent, kind: "note" | "folder", id: string) {
    e.preventDefault();
    ui.noteMenu = { kind, id, x: e.clientX, y: e.clientY };
  }
</script>

{#each folder.folders as f (f.path)}
  {@const open = !ui.collapsed.includes(f.path)}
  <details {open} ontoggle={(e) => ui.setCollapsed(f.path, !e.currentTarget.open)}>
    <summary data-path={f.path} class:active={f.path === router.currentFolder} oncontextmenu={(e) => menu(e, "folder", f.path)}>
      <ChevronRight class="tree-chevron" {...icon} />
      {#if open}<FolderOpen class="tree-icon" {...icon} />{:else}<FolderIcon class="tree-icon" {...icon} />{/if}
      <span class="tree-name" title={`${f.title}\nпапка ${f.path}`}>{f.title}</span>
      <span class="tree-count">{countNotes(f)}</span>
    </summary>
    <div class="items"><TreeFolder folder={f} /></div>
  </details>
{/each}
{#each folder.notes as n (n.id)}
  <a href={noteHref(n.id)} data-id={n.id} class:book={n.kind === "book"} class:active={n.id === router.currentId} oncontextmenu={(e) => menu(e, "note", n.id)}>
    {#if n.kind === "book"}<BookIcon class="tree-icon" {...icon} />{:else}<FileText class="tree-icon" {...icon} />{/if}
    <span class="tree-name">{n.title}</span>
  </a>
{/each}
