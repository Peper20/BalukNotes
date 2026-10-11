<!--
  The "Синхронизация" group of the settings window: the storage server
  account (sign-in, sign-out), the vaults of this device and the server with a
  switch each, the state in words and a "sync now" button. It is not a schema
  setting, so it is its own component in the same window (it sits inside the
  settings form: no nested form, no submit buttons, Enter is handled by hand).
  The state and the requests - lib/state/sync.svelte.ts.
-->
<script lang="ts">
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import type { SyncVault } from "../lib/api";
  import { settings } from "../lib/state";
  import { sync } from "../lib/state/sync.svelte";
  import { conflictText, heldText, isInsecure, serverLabel, serverOnly, stateText } from "../lib/sync";
  import { ui } from "../lib/ui.svelte";

  $effect(() => sync.watch(ui.settingsOpen));

  const status = $derived(sync.status);
  /** The fields of the form; empty until the person types, the account's own values show through (after "session ended"). */
  let serverInput = $state<string | null>(null);
  let loginInput = $state<string | null>(null);
  let password = $state("");
  const server = $derived(serverInput ?? status?.server ?? "");
  const login = $derived(loginInput ?? status?.login ?? "");
  const showForm = $derived(status != null && (!status.signed_in || status.session_ended));
  const canSubmit = $derived(!sync.loginBusy && server.trim() !== "" && login.trim() !== "" && password !== "");
  const insecure = $derived(isInsecure(showForm ? server : status?.server) || (status?.signed_in === true && !status.session_ended && sync.insecure));
  /** The list is for an account, or for vaults still linked after sign-out (their state, switching off). */
  const showVaults = $derived(status != null && (status.signed_in || status.vaults.some((v) => v.linked)));
  const canLink = $derived(status?.signed_in === true && !status.session_ended);
  const icon = { size: 16, strokeWidth: 1.75, "aria-hidden": true } as const;

  async function submit() {
    if (!canSubmit) return;
    if (await sync.login(server, login, password)) {
      password = "";
      serverInput = loginInput = null;
    }
  }

  /** Enter in a field: the sign-in (the settings form must not take it for its own submit). */
  function onKeydown(e: KeyboardEvent) {
    if (e.key !== "Enter") return;
    e.preventDefault();
    void submit();
  }

  async function logout() {
    await sync.logout();
    serverInput = loginInput = null;
    password = "";
  }

  async function toggle(e: Event & { currentTarget: HTMLInputElement }, name: string) {
    const box = e.currentTarget;
    await sync.setLinked(name, box.checked);
    // A failed request leaves the checkbox where the person put it: back to what the core says.
    box.checked = sync.status?.vaults.find((v) => v.name === name)?.linked ?? false;
  }

  function rowText(v: SyncVault): string {
    switch (sync.busy[v.name]) {
      case "link":
        return serverOnly(v) ? "скачивание…" : "подключение…";
      case "unlink":
        return "отключение…";
      case "now":
        return "идёт синхронизация";
      case "confirm":
        return "удаление…";
      case "restore":
        return "возвращаем файлы…";
      default:
        return stateText(v, sync.now);
    }
  }
</script>

<fieldset class="sync" id="sync-settings">
  <legend>Синхронизация</legend>
  {#if sync.unavailable}
    <p class="sync-note">Синхронизация недоступна на этом сервере</p>
  {:else if status == null}
    <p class="sync-note" role={sync.failure ? "alert" : undefined}>{sync.failure ? `Не удалось узнать состояние: ${sync.failure}` : "Загрузка…"}</p>
  {:else}
    {#if status.signed_in && !status.session_ended}
      <div class="sync-account">
        <span class="sync-who">{status.login} на {serverLabel(status.server)}</span>
        <button type="button" class="sync-button" onclick={logout}>Выйти</button>
      </div>
      {#if status.server_error}
        <p class="sync-note" title={status.server_error}>Сервер недоступен. Заметки на устройстве работают, синхронизация продолжится, когда связь вернётся</p>
      {/if}
      {#if sync.logoutError}<p class="sync-error" role="alert">{sync.logoutError}</p>{/if}
    {:else if status.session_ended}
      <p class="sync-warning" role="note">Сессия закончилась - войдите снова</p>
    {/if}

    {#if showForm}
      <div class="sync-form">
        <label for="sync-server">Сервер</label>
        <input
          id="sync-server"
          type="text"
          value={server}
          oninput={(e) => (serverInput = e.currentTarget.value)}
          onkeydown={onKeydown}
          placeholder="notes.example.org"
          autocomplete="url"
          autocapitalize="off"
          spellcheck="false"
        />
        <label for="sync-login">Логин</label>
        <input
          id="sync-login"
          type="text"
          value={login}
          oninput={(e) => (loginInput = e.currentTarget.value)}
          onkeydown={onKeydown}
          maxlength="64"
          autocomplete="username"
          autocapitalize="off"
          spellcheck="false"
        />
        <label for="sync-password">Пароль</label>
        <input id="sync-password" type="password" bind:value={password} onkeydown={onKeydown} autocomplete="current-password" />
        <div class="sync-submit">
          <button type="button" class="sync-button primary" disabled={!canSubmit} onclick={submit}>{sync.loginBusy ? "Вход…" : "Войти"}</button>
        </div>
        {#if sync.loginError}<p class="sync-error" role="alert">{sync.loginError}</p>{/if}
      </div>
    {/if}
    {#if insecure}
      <p class="sync-warning" role="note">Соединение без шифрования: пароль и заметки идут по сети открытым текстом</p>
    {/if}

    {#if showVaults}
      <ul class="sync-vaults" aria-label="Хранилища">
        {#each status.vaults as v (v.name)}
          {@const conflicts = conflictText(v, settings.values["device.sync_prefer"])}
          <li class="sync-vault" data-vault={v.name} data-state={v.state}>
            <span class="sync-name">{v.name}</span>
            <label class="sync-switch">
              <input
                type="checkbox"
                checked={v.linked}
                disabled={Boolean(sync.busy[v.name]) || (!v.linked && !canLink)}
                onchange={(e) => toggle(e, v.name)}
              />
              синхронизировать
            </label>
            <span class="sync-state" data-kind={sync.busy[v.name] ? "busy" : v.linked ? v.state : "off"}>{rowText(v)}</span>
            {#if v.linked}
              <button
                type="button"
                class="icon sync-now"
                title="Синхронизировать сейчас"
                aria-label="Синхронизировать сейчас"
                disabled={Boolean(sync.busy[v.name]) || v.state === "syncing"}
                onclick={() => sync.syncNow(v.name)}
              >
                <RefreshCw {...icon} />
              </button>
            {/if}
            {#if heldText(v)}
              <span class="sync-extra sync-held" role="alert">{heldText(v)}</span>
              <span class="sync-extra sync-held-actions">
                <button type="button" class="sync-button" disabled={Boolean(sync.busy[v.name])} onclick={() => sync.confirmDeletion(v.name)}>
                  {v.held?.side === "server" ? "Удалить на сервере" : "Убрать с устройства"}
                </button>
                {#if v.held?.side === "server"}
                  <button type="button" class="sync-button primary" disabled={Boolean(sync.busy[v.name])} onclick={() => sync.restoreFiles(v.name)}>
                    Вернуть файлы с сервера
                  </button>
                {/if}
              </span>
            {/if}
            {#if conflicts}<span class="sync-extra">{conflicts}</span>{/if}
            {#if sync.rowError[v.name]}<span class="sync-extra sync-error" role="alert">{sync.rowError[v.name]}</span>{/if}
          </li>
        {:else}
          <li class="sync-note">Пока нет ни одного хранилища</li>
        {/each}
      </ul>
    {/if}
  {/if}
</fieldset>
