<!--
  The vault picker: the address does not name a vault, and there is no last
  opened one in this browser (the first start, the vault was deleted). There
  is no default vault: the user picks or creates (and names) one, even the
  first.
-->
<script lang="ts">
  import Library from "@lucide/svelte/icons/library";
  import { onMount } from "svelte";
  import { api, ApiError, type VaultsResponse } from "../lib/api";
  import { vaultHref } from "../lib/boot";
  import { settings } from "../lib/state/settings.svelte";

  let { list }: { list: VaultsResponse } = $props();
  let name = $state("");
  let error = $state<string | null>(null);
  let busy = $state(false);
  const first = $derived(list.vaults.length === 0);

  onMount(() => {
    // The theme as in the app; no settings - by the system (theme CSS).
    void settings
      .load()
      .catch(() => {})
      .finally(() => (document.documentElement.dataset.state = "ready"));
  });
  $effect(() => settings.apply(document.documentElement));

  async function create(e: SubmitEvent) {
    e.preventDefault();
    const wanted = name.trim();
    if (!wanted || busy) return;
    busy = true;
    error = null;
    try {
      await api.createVault(wanted);
      location.assign(vaultHref(wanted));
    } catch (err) {
      error = err instanceof ApiError ? err.message : String(err);
      busy = false;
    }
  }
</script>

<main class="vault-picker" id="vault-picker">
  <h1>{first ? "Первое хранилище" : "Хранилища"}</h1>
  {#if first}
    <p class="vault-picker-lead">Заметки лежат в хранилищах — папках со своим именем. Назовите первое хранилище.</p>
  {:else}
    <p class="vault-picker-lead">Какое хранилище открыть?</p>
    <ul class="vault-picker-list">
      {#each list.vaults as vault (vault)}
        <li><a href={vaultHref(vault)}><Library size={18} strokeWidth={1.75} aria-hidden="true" />{vault}</a></li>
      {/each}
    </ul>
  {/if}
  {#if list.can_create}
    <form class="vault-picker-new" onsubmit={create}>
      <label for="vault-picker-name">{first ? "Название" : "Новое хранилище"}</label>
      <div class="vault-picker-row">
        <!-- svelte-ignore a11y_autofocus -->
        <input id="vault-picker-name" type="text" bind:value={name} maxlength="64" autocomplete="off" spellcheck="false" autofocus={first} required />
        <button type="submit" disabled={busy || !name.trim()}>Создать</button>
      </div>
      {#if error}<p class="dialog-error" role="alert">{error}</p>{/if}
    </form>
  {/if}
</main>
