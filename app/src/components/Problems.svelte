<!-- Ошибки и предупреждения сборки над заметкой. -->
<script lang="ts">
  import type { Diagnostic, NotePage } from "../lib/api";

  let { page }: { page: NotePage } = $props();

  const where = (d: Diagnostic) => [d.file, d.line, d.column].filter((x) => x != null).join(":");
</script>

{#snippet item(d: Diagnostic)}
  <li>
    {#if d.file}<span class="where">{where(d)} </span>{/if}
    <code>{d.message}</code>
    {#each d.hints as hint}<div class="where">подсказка: {hint}</div>{/each}
  </li>
{/snippet}

{#if page.errors.length || page.warnings.length}
  <section class="problems" id="problems">
    {#if page.errors.length}
      <div class="errors">
        <h3>{page.rendered ? "Не собралось — показана прошлая версия" : "Не собралось"}</h3>
        <ul>{#each page.errors as d}{@render item(d)}{/each}</ul>
      </div>
    {/if}
    {#if page.warnings.length}
      <details>
        <summary>Предупреждения: {page.warnings.length}</summary>
        <ul>{#each page.warnings as d}{@render item(d)}{/each}</ul>
      </details>
    {/if}
  </section>
{/if}
