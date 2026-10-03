<!-- Боковая панель: дерево заметок и книг, внизу — хранилище (меню хранилищ). -->
<script lang="ts">
  import { notes, router } from "../lib/state";
  import { tick, untrack } from "svelte";
  import { ancestors } from "../lib/tree";
  import { ui } from "../lib/ui.svelte";
  import NoteMenu from "./NoteMenu.svelte";
  import TreeFolder from "./TreeFolder.svelte";
  import VaultMenu from "./VaultMenu.svelte";

  let nav: HTMLElement | undefined = $state();

  // Открытая заметка или папка видна в дереве: папки на пути раскрыты (при
  // переходе — потом их можно свернуть), строка — в поле зрения дерева (не
  // всей страницы).
  $effect(() => {
    const id = router.currentId;
    const folder = router.currentFolder;
    if (id == null && folder == null) return;
    const open = id != null ? ancestors(id) : ancestors(`${folder}/_`).slice(0, -1);
    untrack(() => open.forEach((path) => ui.setCollapsed(path, false)));
    void tick().then(() => {
      const row = nav?.querySelector<HTMLElement>(id != null ? `a[data-id="${CSS.escape(id)}"]` : `summary[data-path="${CSS.escape(folder!)}"]`);
      if (!nav || !row) return;
      const [box, r] = [nav.getBoundingClientRect(), row.getBoundingClientRect()];
      if (r.top < box.top + 40 || r.bottom > box.bottom) nav.scrollTop += r.top - box.top - box.height / 3;
    });
  });
</script>

<aside class="sidebar" id="sidebar">
  <nav class="tree" id="tree" aria-label="Заметки" bind:this={nav}><TreeFolder folder={notes.tree} /></nav>
  <VaultMenu />
  <NoteMenu />
</aside>
