<!-- Папка дерева заметок: подпапки, затем заметки. Рекурсивно. -->
<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { noteHref } from "../lib/ids";
  import type { Folder } from "../lib/tree";
  import { ui } from "../lib/ui.svelte";
  import TreeFolder from "./TreeFolder.svelte";

  let { folder }: { folder: Folder } = $props();
</script>

{#each folder.folders as f (f.path)}
  <details
    open={!ui.collapsed.includes(f.path)}
    ontoggle={(e) => ui.setCollapsed(f.path, !e.currentTarget.open)}
  >
    <summary>{f.name}</summary>
    <div class="items"><TreeFolder folder={f} /></div>
  </details>
{/each}
{#each folder.notes as n (n.id)}
  <a href={noteHref(n.id)} data-id={n.id} class:book={n.kind === "book"} class:active={n.id === app.currentId}>{n.name}</a>
{/each}
