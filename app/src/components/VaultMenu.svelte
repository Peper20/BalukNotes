<!--
  Низ боковой панели (как в Obsidian): хранилище и меню хранилищ — перейти в
  другое (с перезагрузкой: вкладки и места чтения у каждого свои), создать
  новое, переименовать или удалить открытое. Рядом — главная и граф хранилища.
-->
<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import ChevronsUpDown from "@lucide/svelte/icons/chevrons-up-down";
  import House from "@lucide/svelte/icons/house";
  import Library from "@lucide/svelte/icons/library";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Plus from "@lucide/svelte/icons/plus";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import { Icon } from "@lucide/svelte";
  import { api, type VaultsResponse } from "../lib/api";
  import { graphHref, homeHref } from "../lib/ids";
  import { graphIcon } from "../lib/icons";
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

  function dialog(which: "new" | "rename" | "delete") {
    open = false;
    if (which === "new") ui.vaultNewOpen = true;
    else ui.vaultEdit = which;
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
<div class="sidebar-foot" bind:this={root} onkeydown={onKeydown}>
  <button
    type="button"
    class="vault-switch"
    id="vault-switch"
    title="Хранилища"
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={toggle}
  >
    <Library {...icon} />
    <span class="vault-switch-name" id="vault-name">{vault()}</span>
    <ChevronsUpDown {...icon} />
  </button>
  <a href={homeHref()} class="icon vault-home" title="Главная хранилища «{vault()}»" aria-label="Главная хранилища"><House {...icon} /></a>
  <a href={graphHref()} class="icon vault-home" id="open-graph" title="Граф хранилища «{vault()}» (G)" aria-label="Граф хранилища"><Icon iconNode={graphIcon} {...icon} /></a>
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
        <div class="vault-menu-actions">
          <button type="button" role="menuitem" id="vault-create" onclick={() => dialog("new")}>
            <span class="vault-menu-mark"><Plus {...icon} /></span>
            <span class="vault-menu-name">Новое хранилище…</span>
          </button>
          <button type="button" role="menuitem" id="vault-rename" onclick={() => dialog("rename")}>
            <span class="vault-menu-mark"><Pencil {...icon} /></span>
            <span class="vault-menu-name">Переименовать «{vault()}»…</span>
          </button>
          <button type="button" role="menuitem" id="vault-delete" class="danger" onclick={() => dialog("delete")}>
            <span class="vault-menu-mark"><Trash2 {...icon} /></span>
            <span class="vault-menu-name">Удалить «{vault()}»…</span>
          </button>
        </div>
      {/if}
    </div>
  {/if}
</div>
