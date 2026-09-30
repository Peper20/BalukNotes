<!--
  Открытое хранилище: переименовать (папку) или удалить целиком в корзину
  системы. Вкладки и места чтения в этом браузере — вслед за ним; потом —
  перезагрузка: под новым именем или экран выбора хранилища.
-->
<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import { api, ApiError } from "../lib/api";
  import { vaultMoved } from "../lib/boot";
  import { plural } from "../lib/plural";
  import { notes } from "../lib/state";
  import { ui } from "../lib/ui.svelte";
  import { splitVaultPath, vault, vaultBase } from "../lib/vault";

  let dialog: HTMLDialogElement | undefined = $state();
  let cancel: HTMLButtonElement | undefined = $state();
  let name = $state("");
  let error = $state<string | null>(null);
  let busy = $state(false);

  const current = $derived(vault() ?? "");
  const count = $derived(notes.all.length);

  $effect(() => {
    if (!dialog) return;
    if (ui.vaultEdit && !dialog.open) {
      name = current;
      error = null;
      busy = false;
      dialog.showModal();
      // Удаление: «Отмена» — по умолчанию, Enter не удалит случайно.
      if (ui.vaultEdit === "delete") cancel?.focus();
    } else if (!ui.vaultEdit && dialog.open) dialog.close();
  });

  const close = () => (ui.vaultEdit = null);

  async function run(action: () => Promise<unknown>, then: () => void) {
    if (busy) return;
    busy = true;
    error = null;
    try {
      await action();
      then();
    } catch (err) {
      error = err instanceof ApiError ? err.message : String(err);
      busy = false;
    }
  }

  function rename(e: SubmitEvent) {
    e.preventDefault();
    const wanted = name.trim();
    if (!wanted || wanted === current) return close();
    void run(
      () => api.renameVault(current, wanted),
      () => {
        vaultMoved(current, wanted);
        // Та же страница — под новым именем хранилища.
        const rest = splitVaultPath(location.pathname)?.rest ?? "/";
        location.assign(vaultBase(wanted) + rest + location.search + location.hash);
      },
    );
  }

  function remove() {
    void run(
      () => api.deleteVault(current),
      () => {
        vaultMoved(current, null);
        location.assign("/");
      },
    );
  }
</script>

<dialog class="settings vault-edit" id="vault-edit" bind:this={dialog} onclose={close} aria-labelledby="vault-edit-title">
  {#if ui.vaultEdit === "delete"}
    <header>
      <h2 id="vault-edit-title">Удалить хранилище «{current}»?</h2>
      <button type="button" class="icon" aria-label="Закрыть" onclick={close}><X size={18} strokeWidth={1.75} aria-hidden="true" /></button>
    </header>
    <div class="dialog-body">
      <p>
        Папка хранилища уйдёт в корзину системы целиком{count ? ` (заметок и книг: ${count})` : ""}. Вернуть можно
        оттуда.
      </p>
      {#if error}<p class="dialog-error" role="alert">{error}</p>{/if}
    </div>
    <footer class="dialog-actions">
      <button type="button" bind:this={cancel} onclick={close}>Отмена</button>
      <button type="button" class="danger" id="vault-delete-confirm" disabled={busy} onclick={remove}>Удалить</button>
    </footer>
  {:else}
    <form onsubmit={rename}>
      <header>
        <h2 id="vault-edit-title">Переименовать хранилище</h2>
        <button type="button" class="icon" aria-label="Закрыть" onclick={close}><X size={18} strokeWidth={1.75} aria-hidden="true" /></button>
      </header>
      <div class="dialog-body">
        <label for="vault-edit-name">Название</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="vault-edit-name" type="text" bind:value={name} maxlength="64" autocomplete="off" spellcheck="false" autofocus required />
        <p class="dialog-hint">Переименуется и папка хранилища. Ссылки <code>#see</code> в заметках не меняются: они внутри хранилища.</p>
        {#if error}<p class="dialog-error" role="alert">{error}</p>{/if}
      </div>
      <footer class="dialog-actions">
        <button type="button" onclick={close}>Отмена</button>
        <button type="submit" class="primary" disabled={busy || !name.trim()}>Переименовать</button>
      </footer>
    </form>
  {/if}
</dialog>
