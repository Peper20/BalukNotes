<!-- «Ссылаются сюда»: кто ссылается на заметку — из индекса ссылок (без компиляции). -->
<script lang="ts">
  import { api, type LinksResponse } from "../lib/api";
  import { app } from "../lib/app.svelte";
  import { graphHref, noteHref, splitId } from "../lib/ids";

  let { id }: { id: string } = $props();
  let links = $state.raw<LinksResponse | null>(null);

  $effect(() => {
    const current = id;
    links = null;
    api
      .links(current)
      .then((l) => {
        if (current === id) links = l;
      })
      .catch(() => {});
  });
</script>

{#if links?.backlinks.length}
  <section class="backlinks" id="backlinks">
    <h2>Ссылаются сюда · {links.backlinks.length} <a class="backlinks-graph" href={graphHref(id)}>на графе</a></h2>
    <ul>
      {#each links.backlinks as b}
        {@const note = app.notes.find((n) => n.id === b.from)}
        <li>
          <a href={noteHref(b.from)}>{note?.name ?? splitId(b.from).name}</a>
          {#if note?.folder}<span class="anchor"> · {note.folder}</span>{/if}
          {#if b.anchor}<span class="anchor"> → «{b.anchor}»</span>{/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}
