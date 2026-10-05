<!--
  "Ссылаются сюда": who links to the note, from the link index (no
  compiling). A link into a section shows its heading as in the outline
  (heading HTML from the core: formulas, emphasis); the section is not on the
  page (another book chapter) - the heading text; not found - the anchor as
  written.
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
   * The section heading on the page: the anchor is its `id` (a label) or the
   * text slug (`data-k-anchor`), as a link finds it; otherwise the section
   * found by the core.
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
