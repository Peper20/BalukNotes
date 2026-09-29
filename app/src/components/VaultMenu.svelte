<!--
  Шапка боковой панели: имя хранилища (ссылка на его главную) и меню
  хранилищ — перейти в другое (с перезагрузкой: вкладки и места чтения у
  каждого свои) или создать новое.
-->
<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import ChevronsUpDown from "@lucide/svelte/icons/chevrons-up-down";
  import Plus from "@lucide/svelte/icons/plus";
  import { api, type VaultsResponse } from "../lib/api";
  import { homeHref } from "../lib/ids";
  import { ui } from "../lib/ui.svelte";
  import { vault, vaultHome } from "../lib/vault";

  let open = $state(false);
  let list = $state<VaultsResponse | null>(null);
  let failed = $state(false);
  let root: HTMLElement | undefined = $state();
  const icon = { size: 16, strokeWidth: 1.75, "aria-hidden": true } as const;

  async function toggle() {
    open = !open;
    if (!open) return;
    failed = false;
    try {
      list = await api.vaults();
    } catch {
      failed = true;
    }
  }

  function create() {
    open = false;
    ui.vaultNewOpen = true;
  }

  function onPointerDown(e: PointerEvent) {
    if (open && !root?.contains(e.target as Node)) open = false;
  }

  function onKeydown(e: KeyboardEvent) {
    if (open && e.key === "Escape") {
      e.stopPropagation();
      open = false;
    }
  }
</script>

<svelte:window onpointerdown={onPointerDown} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="sidebar-head" bind:this={root} onkeydown={onKeydown}>
  <a href={homeHref()} class="brand" id="vault-name" title="Главная хранилища «{vault()}»">{vault()}</a>
  <button type="button" class="icon" id="vault-switch" title="Хранилища" aria-label="Хранилища" aria-haspopup="menu" aria-expanded={open} onclick={toggle}>
    <ChevronsUpDown {...icon} />
  </button>
  {#if open}
    <div class="vault-menu" id="vault-menu" role="menu" aria-label="Хранилища">
      {#if failed}<p class="vault-menu-note">Сервер не ответил</p>{/if}
      {#each list?.vaults ?? [] as name (name)}
        {@const current = name === vault()}
        <a role="menuitem" href={vaultHome(name)} class:current aria-current={current ? "true" : undefined}>
          <span class="vault-menu-mark">{#if current}<Check {...icon} />{/if}</span>
          <span class="vault-menu-name">{name}</span>
        </a>
      {/each}
      {#if list?.can_create}
        <button type="button" role="menuitem" id="vault-create" onclick={create}>
          <span class="vault-menu-mark"><Plus {...icon} /></span>
          <span class="vault-menu-name">Новое хранилище…</span>
        </button>
      {/if}
    </div>
  {/if}
</div>
