<!--
  The "собирается" placeholder in place of the note. It appears with a delay
  (CSS), so a note from the cache replaces it before it becomes visible; a
  long build shows a seconds counter and an explanation.
-->
<script lang="ts">
  import { notes } from "../lib/state";

  let { id, since }: { id: string; since: number } = $props();
  let now = $state(Date.now());

  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(timer);
  });

  const note = $derived(notes.all.find((n) => n.id === id));
  const seconds = $derived(Math.round((now - since) / 1000));
</script>

<div class="loading" role="status">
  <div class="loading-name">{notes.title(id)}</div>
  <div class="loading-bar"></div>
  <p class="loading-text">Собирается… {seconds > 0 ? `${seconds} с` : ""}</p>
  {#if seconds >= 2}
    <p class="loading-hint">
      {note?.kind === "book"
        ? "Книга собирается целиком в двух темах — первый раз это несколько секунд. Дальше она открывается сразу, даже после перезапуска."
        : "Первая сборка заметки; дальше она открывается сразу."}
    </p>
  {/if}
</div>
