<!-- Previous / next book chapter under the chapter text. -->
<script lang="ts">
  import { ui } from "../lib/ui.svelte";

  const book = $derived(ui.book);
  const prev = $derived(book && ui.chapter > 0 ? book.chapters[ui.chapter - 1] : undefined);
  const next = $derived(book ? book.chapters[ui.chapter + 1] : undefined);
</script>

{#snippet link(c: NonNullable<typeof prev>, cls: string, label: string)}
  <a class={cls} href="#{encodeURIComponent(c.id)}">
    <small>{label}</small>{c.num ? `${c.num}. ` : ""}{c.title}
  </a>
{/snippet}

{#if book}
  <nav class="chapter-nav" aria-label="Главы">
    {#if prev}{@render link(prev, "prev", "← предыдущая глава")}{/if}
    {#if next}{@render link(next, "next", "следующая глава →")}{/if}
  </nav>
{/if}
