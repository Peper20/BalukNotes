<!--
  Удалить заметку? — подтверждение: что уйдёт в корзину (книга — папкой) и
  сколько заметок на неё ссылается (их ссылки станут битыми).
-->
<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import { api, ApiError } from "../lib/api";
  import { deleteNote } from "../lib/commands.svelte";
  import { plural } from "../lib/plural";
  import { notes } from "../lib/state";
  import { ui } from "../lib/ui.svelte";

  let dialog: HTMLDialogElement | undefined = $state();
  let cancel: HTMLButtonElement | undefined = $state();
  /** Сколько других заметок ссылается сюда; null — ещё не знаем. */
  let linked = $state<number | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  const id = $derived(ui.deleting);
  const book = $derived(notes.byId(id)?.kind === "book");
  const name = $derived(id ? notes.title(id) : "");

  $effect(() => {
    if (!dialog) return;
    const target = id;
    if (target && !dialog.open) {
      linked = null;
      error = null;
      busy = false;
      dialog.showModal();
      // «Отмена» — по умолчанию: Enter не удалит случайно.
      cancel?.focus();
      api
        .links(target)
        .then((l) => {
          if (ui.deleting === target) linked = new Set(l.backlinks.map((b) => b.from).filter((f) => f !== target)).size;
        })
        .catch(() => {});
    } else if (!target && dialog.open) dialog.close();
  });

  async function confirm() {
    if (!id || busy) return;
    busy = true;
    try {
      await deleteNote(id);
      ui.deleting = null;
    } catch (err) {
      error = err instanceof ApiError ? err.message : String(err);
      busy = false;
    }
  }
</script>

<dialog class="settings note-delete" id="note-delete" bind:this={dialog} onclose={() => (ui.deleting = null)} aria-labelledby="note-delete-title">
  <header>
    <h2 id="note-delete-title">Удалить {book ? "книгу" : "заметку"} «{name}»?</h2>
    <button type="button" class="icon" aria-label="Закрыть" onclick={() => (ui.deleting = null)}><X size={18} strokeWidth={1.75} aria-hidden="true" /></button>
  </header>
  <div class="dialog-body">
    <p>
      {#if book}Книга уйдёт в корзину вместе со своей папкой — со всеми главами.{:else}Заметка уйдёт в корзину.{/if}
      Вернуть можно оттуда.
    </p>
    <p class="dialog-hint"><code>{id}</code></p>
    {#if linked}<p>На неё {plural(linked, "ссылается", "ссылаются", "ссылаются")} {linked} {plural(linked, "заметка", "заметки", "заметок")} — эти ссылки станут битыми.</p>{/if}
    {#if error}<p class="dialog-error" role="alert">{error}</p>{/if}
  </div>
  <footer class="dialog-actions">
    <button type="button" bind:this={cancel} onclick={() => (ui.deleting = null)}>Отмена</button>
    <button type="button" class="danger" id="note-delete-confirm" disabled={busy} onclick={confirm}>Удалить</button>
  </footer>
</dialog>
