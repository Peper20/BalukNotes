<!-- Боковая панель: дерево заметок и книг, внизу — хранилище (меню хранилищ). -->
<script lang="ts">
  import { notes, router } from "../lib/state";
  import { tick, untrack } from "svelte";
  import { ancestors, buildTree } from "../lib/tree";
  import { ui } from "../lib/ui.svelte";
  import NoteMenu from "./NoteMenu.svelte";
  import TreeFolder from "./TreeFolder.svelte";
  import VaultMenu from "./VaultMenu.svelte";

  const tree = $derived(buildTree(notes.all, (path) => notes.folderTitle(path)));
  let nav: HTMLElement | undefined = $state();

  // Открытая заметка видна в дереве: её папки раскрыты (при переходе — потом
  // их можно свернуть), строка — в поле зрения дерева (не всей страницы).
  $effect(() => {
    const id = router.currentId;
    if (!id) return;
    untrack(() => ancestors(id).forEach((path) => ui.setCollapsed(path, false)));
    void tick().then(() => {
      const link = nav?.querySelector<HTMLElement>(`a[data-id="${CSS.escape(id)}"]`);
      if (!nav || !link) return;
      const [box, row] = [nav.getBoundingClientRect(), link.getBoundingClientRect()];
      if (row.top < box.top + 40 || row.bottom > box.bottom) nav.scrollTop += row.top - box.top - box.height / 3;
    });
  });
</script>

<aside class="sidebar" id="sidebar">
  <nav class="tree" id="tree" aria-label="Заметки" bind:this={nav}><TreeFolder folder={tree} /></nav>
  <VaultMenu />
  <NoteMenu />
</aside>
