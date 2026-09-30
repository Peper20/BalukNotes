<!--
  Удалить заметку (папку)? — подтверждение: что уйдёт в корзину (книга —
  папкой, папка — со всем, что в ней) и сколько других заметок ссылается туда
  (их ссылки станут битыми).
-->
<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import { api, ApiError } from "../lib/api";
  import { deleteFolder, deleteNote } from "../lib/commands.svelte";
  import { plural } from "../lib/plural";
  import { notes } from "../lib/state";
  import { ui, type TreeItem } from "../lib/ui.svelte";

  let dialog: HTMLDialogElement | undefined = $state();
  let cancel: HTMLButtonElement | undefined = $state();
  /** Сколько других заметок ссылается сюда; null — ещё не знаем. */
  let linked = $state<number | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  const target = $derived(ui.deleting);
  const id = $derived(target?.id ?? null);
  const folder = $derived(target?.kind === "folder");
  const book = $derived(!folder && notes.byId(id)?.kind === "book");
  const name = $derived(!target ? "" : folder ? notes.folderTitle(target.id) : notes.title(target.id));
  /** Заметок и книг в удаляемой папке. */
  const count = $derived(folder && id ? notes.all.filter((n) => n.id.startsWith(`${id}/`)).length : 0);

  /** Сколько других заметок ссылается на заметку или внутрь папки. */
  async function linkedTo(t: TreeItem): Promise<number> {
    if (t.kind === "note") return new Set((await api.links(t.id)).backlinks.map((b) => b.from).filter((f) => f !== t.id)).size;
    const inside = (n: string) => n.startsWith(`${t.id}/`);
    return new Set((await api.graph()).edges.filter((e) => inside(e.to) && !inside(e.from)).map((e) => e.from)).size;
  }

  $effect(() => {
    if (!dialog) return;
    const t = target;
    if (t && !dialog.open) {
      linked = null;
      error = null;
      busy = false;
      dialog.showModal();
      // «Отмена» — по умолчанию: Enter не удалит случайно.
      cancel?.focus();
      linkedTo(t)
        .then((n) => {
          if (ui.deleting === t) linked = n;
        })
        .catch(() => {});
    } else if (!t && dialog.open) dialog.close();
  });

  async function confirm() {
    if (!target || busy) return;
    busy = true;
    try {
      await (target.kind === "folder" ? deleteFolder(target.id) : deleteNote(target.id));
      ui.deleting = null;
    } catch (err) {
      error = err instanceof ApiError ? err.message : String(err);
      busy = false;
    }
  }
</script>

<dialog class="settings note-delete" id="note-delete" bind:this={dialog} onclose={() => (ui.deleting = null)} aria-labelledby="note-delete-title">
  <header>
    <h2 id="note-delete-title">Удалить {folder ? "папку" : book ? "книгу" : "заметку"} «{name}»?</h2>
    <button type="button" class="icon" aria-label="Закрыть" onclick={() => (ui.deleting = null)}><X size={18} strokeWidth={1.75} aria-hidden="true" /></button>
  </header>
  <div class="dialog-body">
    <p>
      {#if folder}Папка уйдёт в корзину со всем, что в ней: {count} {plural(count, "заметка", "заметки", "заметок")}.{:else if book}Книга уйдёт в корзину вместе со своей папкой — со всеми главами.{:else}Заметка уйдёт в корзину.{/if}
      Вернуть можно оттуда.
    </p>
    <p class="dialog-hint"><code>{id}</code></p>
    {#if linked}<p>{folder ? "На заметки из неё" : "На неё"} {plural(linked, "ссылается", "ссылаются", "ссылаются")} {linked} {plural(linked, "заметка", "заметки", "заметок")} — эти ссылки станут битыми.</p>{/if}
    {#if error}<p class="dialog-error" role="alert">{error}</p>{/if}
  </div>
  <footer class="dialog-actions">
    <button type="button" bind:this={cancel} onclick={() => (ui.deleting = null)}>Отмена</button>
    <button type="button" class="danger" id="note-delete-confirm" disabled={busy} onclick={confirm}>Удалить</button>
  </footer>
</dialog>
