<!-- Боковая панель: хранилище (меню хранилищ), дерево заметок и книг. -->
<script lang="ts">
  import { notes, router } from "../lib/state";
  import { tick, untrack } from "svelte";
  import { ancestors, buildTree } from "../lib/tree";
  import { ui } from "../lib/ui.svelte";
  import NoteMenu from "./NoteMenu.svelte";
  import TreeFolder from "./TreeFolder.svelte";
  import VaultMenu from "./VaultMenu.svelte";

  const tree = $derived(buildTree(notes.all));
  let aside: HTMLElement | undefined = $state();

  // Открытая заметка видна в дереве: её папки раскрыты (при переходе — потом
  // их можно свернуть), строка — в поле зрения панели (не всей страницы).
  $effect(() => {
    const id = router.currentId;
    if (!id) return;
    untrack(() => ancestors(id).forEach((path) => ui.setCollapsed(path, false)));
    void tick().then(() => {
      const link = aside?.querySelector<HTMLElement>(`a[data-id="${CSS.escape(id)}"]`);
      if (!aside || !link) return;
      const [box, row] = [aside.getBoundingClientRect(), link.getBoundingClientRect()];
      if (row.top < box.top + 40 || row.bottom > box.bottom) aside.scrollTop += row.top - box.top - box.height / 3;
    });
  });
</script>

<aside class="sidebar" id="sidebar" bind:this={aside}>
  <VaultMenu />
  <nav class="tree" id="tree" aria-label="Заметки"><TreeFolder folder={tree} /></nav>
  <NoteMenu />
</aside>
