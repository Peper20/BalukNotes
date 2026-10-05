<!--
  Renames a note (book) or a folder: the new title -> the file name from it
  and which notes get fixed (#see links to it) - a plan from the server, as
  you type; applied by "Переименовать".
-->
<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import { api, ApiError, type RenamePlan } from "../lib/api";
  import { renameItem } from "../lib/commands.svelte";
  import { plural } from "../lib/plural";
  import { notes } from "../lib/state";
  import { ui } from "../lib/ui.svelte";

  /** How many notes with links to show as a list. */
  const SHOWN = 6;

  let dialog: HTMLDialogElement | undefined = $state();
  let input: HTMLInputElement | undefined = $state();
  let title = $state("");
  let plan = $state<RenamePlan | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  const target = $derived(ui.renaming);
  const folder = $derived(target?.kind === "folder");
  const book = $derived(!folder && notes.byId(target?.id ?? null)?.kind === "book");
  const current = $derived(!target ? "" : folder ? notes.folderTitle(target.id) : notes.title(target.id));
  const what = $derived(folder ? "папку" : book ? "книгу" : "заметку");
  /** The plan for what is in the input now. */
  const fresh = $derived(plan != null && plan.title === title.split(/\s+/).filter(Boolean).join(" "));
  const links = $derived(plan?.links.reduce((sum, l) => sum + l.count, 0) ?? 0);

  $effect(() => {
    if (!dialog) return;
    if (target && !dialog.open) {
      title = current;
      plan = null;
      error = null;
      busy = false;
      dialog.showModal();
      input?.select();
    } else if (!target && dialog.open) dialog.close();
  });

  // The plan as you type, with a pause; a stale request is cancelled.
  $effect(() => {
    const t = target;
    const wanted = title;
    if (!t) return;
    const ctrl = new AbortController();
    const timer = setTimeout(() => {
      api
        .rename({ kind: t.kind, id: t.id, title: wanted, apply: false }, ctrl.signal)
        .then((p) => {
          plan = p;
          error = null;
        })
        .catch((err) => {
          if (ctrl.signal.aborted) return;
          plan = null;
          error = err instanceof ApiError ? err.message : String(err);
        });
    }, 200);
    return () => {
      clearTimeout(timer);
      ctrl.abort();
    };
  });

  const close = () => (ui.renaming = null);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!target || busy || !fresh) return;
    if (plan!.from === plan!.to && plan!.title === current) return close();
    busy = true;
    try {
      await renameItem(target.kind, target.id, title);
      close();
    } catch (err) {
      error = err instanceof ApiError ? err.message : String(err);
      busy = false;
    }
  }

  /** A path for display: a note file with `.typ`, a book and a folder with `/`. */
  const shownPath = (path: string) => (folder || book ? `${path}/` : `${path}.typ`);
</script>

<dialog class="settings note-rename" id="note-rename" bind:this={dialog} onclose={close} aria-labelledby="note-rename-title">
  <form onsubmit={submit}>
    <header>
      <h2 id="note-rename-title">Переименовать {what}</h2>
      <button type="button" class="icon" aria-label="Закрыть" onclick={close}><X size={18} strokeWidth={1.75} aria-hidden="true" /></button>
    </header>
    <div class="dialog-body">
      <label for="note-rename-title-input">Название</label>
      <input id="note-rename-title-input" type="text" bind:this={input} bind:value={title} autocomplete="off" spellcheck="false" required />
      {#if plan && fresh}
        <p class="dialog-hint">
          {#if plan.from === plan.to}Имя {folder || book ? "папки" : "файла"} не меняется: <code>{shownPath(plan.to)}</code>
          {:else}<code>{shownPath(plan.from)}</code> → <code>{shownPath(plan.to)}</code>{/if}
        </p>
        {#if plan.links.length}
          <p>
            {plural(links, "Перепишется", "Перепишутся", "Перепишутся")}
            {links}
            {plural(links, "ссылка", "ссылки", "ссылок")} в {plan.links.length}
            {plural(plan.links.length, "заметке", "заметках", "заметках")}:
          </p>
          <ul class="rename-links">
            {#each plan.links.slice(0, SHOWN) as l (l.note)}<li>{notes.title(l.note)}</li>{/each}
            {#if plan.links.length > SHOWN}<li>и ещё {plan.links.length - SHOWN}</li>{/if}
          </ul>
        {:else}
          <p class="dialog-hint">Других заметок со ссылками {folder ? "сюда" : "на неё"} нет.</p>
        {/if}
      {/if}
      {#if error}<p class="dialog-error" role="alert">{error}</p>{/if}
    </div>
    <footer class="dialog-actions">
      <button type="button" onclick={close}>Отмена</button>
      <button type="submit" class="primary" id="note-rename-confirm" disabled={busy || !fresh}>Переименовать</button>
    </footer>
  </form>
</dialog>
