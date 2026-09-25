<!-- Боковая панель: дерево заметок и книг. -->
<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { buildTree } from "../lib/tree";
  import TreeFolder from "./TreeFolder.svelte";

  const tree = $derived(buildTree(app.notes));
</script>

<aside class="sidebar" id="sidebar">
  <div class="sidebar-head"><a href="/" class="brand">Заметки</a></div>
  <nav class="tree" id="tree" aria-label="Заметки"><TreeFolder folder={tree} /></nav>
</aside>
