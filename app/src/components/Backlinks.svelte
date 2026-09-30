<!--
  «Ссылаются сюда»: кто ссылается на заметку — из индекса ссылок (без
  компиляции). Ссылка в раздел — его заголовок как в оглавлении (HTML
  заголовка из ядра: формулы, выделение); раздела нет на странице (другая
  глава книги) — текст заголовка, не нашёлся — якорь как написан.
-->
<script lang="ts">
  import { api, type Backlink, type LinksResponse } from "../lib/api";
  import { notes, reader } from "../lib/state";
  import { graphHref, noteHref } from "../lib/ids";

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

  /**
   * Заголовок раздела на странице: якорь — его `id` (метка) или слаг текста
   * (`data-k-anchor`), как находит ссылка; иначе — раздел, найденный ядром.
   */
  function headingOf(b: Backlink) {
    const headings = reader.page?.rendered?.headings ?? [];
    const wanted = [b.anchor, b.section].filter((a) => a != null);
    for (const a of wanted) {
      const h = headings.find((h) => h.id === a || h.anchor === a);
      if (h) return h;
    }
    return undefined;
  }
</script>

{#if links?.backlinks.length}
  <section class="backlinks" id="backlinks">
    <h2>Ссылаются сюда · {links.backlinks.length} <a class="backlinks-graph" href={graphHref(id)}>на графе</a></h2>
    <ul>
      {#each links.backlinks as b}
        {@const note = notes.all.find((n) => n.id === b.from)}
        {@const h = headingOf(b)}
        <li>
          <a href={noteHref(b.from)}>{notes.title(b.from)}</a>
          {#if note?.folder}<span class="anchor"> · {notes.folderLabel(note.folder)}</span>{/if}
          {#if b.anchor}
            <span class="anchor"> → «{#if h?.html}<span class="backlinks-heading">{@html h.html}</span>{:else}{h?.text ?? b.heading ?? b.anchor}{/if}»</span>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}
