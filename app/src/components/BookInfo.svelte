<!--
  What all chapters of a book share (the root `main.typ`): the title (a link
  to the start) and the book tags, at the top of the outline while reading
  any chapter (user's choice). Under a chapter heading - only its own tags.
-->
<script lang="ts">
  import BookOpen from "@lucide/svelte/icons/book-open";
  import { notes, router } from "../lib/state";
  import { tagHref } from "../lib/ids";
  import { ui } from "../lib/ui.svelte";

  const book = $derived(ui.book && router.currentId ? notes.byId(router.currentId) : undefined);
  const first = $derived(ui.book?.chapters[0]);
</script>

{#if book?.kind === "book"}
  <div class="book-info" role="group" aria-label="Книга">
    <BookOpen size={15} strokeWidth={1.75} aria-hidden="true" />
    {#if first}<a class="book-info-title" href="#{encodeURIComponent(first.id)}" title="К началу книги">{book.title}</a>{:else}<span
        class="book-info-title">{book.title}</span
      >{/if}
    {#if book.tags.length}
      <span class="book-info-tags">
        {#each book.tags as tag (tag)}<a href={tagHref(tag)}>#{tag}</a>{/each}
      </span>
    {/if}
  </div>
{/if}
