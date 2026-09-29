<!-- Новое хранилище: имя → папка в каталоге данных; создано — открыть его. -->
<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import { api, ApiError } from "../lib/api";
  import { ui } from "../lib/ui.svelte";
  import { vaultHome } from "../lib/vault";

  let dialog: HTMLDialogElement | undefined = $state();
  let name = $state("");
  let error = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    if (!dialog) return;
    if (ui.vaultNewOpen && !dialog.open) {
      name = "";
      error = null;
      dialog.showModal();
    } else if (!ui.vaultNewOpen && dialog.open) dialog.close();
  });

  async function create(e: SubmitEvent) {
    e.preventDefault();
    const wanted = name.trim();
    if (!wanted || busy) return;
    busy = true;
    error = null;
    try {
      await api.createVault(wanted);
      // Другое хранилище — с перезагрузкой: вкладки и места чтения у него свои.
      location.assign(vaultHome(wanted));
    } catch (err) {
      error = err instanceof ApiError ? err.message : String(err);
      busy = false;
    }
  }
</script>

<dialog class="settings vault-new" id="vault-new" bind:this={dialog} onclose={() => (ui.vaultNewOpen = false)} aria-labelledby="vault-new-title">
  <form onsubmit={create}>
    <header>
      <h2 id="vault-new-title">Новое хранилище</h2>
      <button type="button" class="icon" aria-label="Закрыть" onclick={() => (ui.vaultNewOpen = false)}><X size={18} strokeWidth={1.75} aria-hidden="true" /></button>
    </header>
    <div class="dialog-body">
      <label for="vault-new-name">Название</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="vault-new-name" type="text" bind:value={name} maxlength="64" autocomplete="off" spellcheck="false" autofocus required />
      <p class="dialog-hint">Заметки хранилища — в папке с тем же именем в каталоге данных (<code>notes info</code>).</p>
      {#if error}<p class="dialog-error" role="alert">{error}</p>{/if}
    </div>
    <footer class="dialog-actions">
      <button type="button" onclick={() => (ui.vaultNewOpen = false)}>Отмена</button>
      <button type="submit" class="primary" disabled={busy || !name.trim()}>Создать</button>
    </footer>
  </form>
</dialog>
