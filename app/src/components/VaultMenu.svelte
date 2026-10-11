<!--
  The bottom of the sidebar (as in Obsidian): the vault and the vault menu -
  go to another one (with a reload: tabs and reading places are per vault),
  create a new one, rename or delete the open one. Next to it - home and the
  vault graph. Another vault opens in this window or in a new one: a click by
  the setting `vaults.open`, Ctrl/Cmd/middle click - a new window, right click -
  a choice (`lib/vault-open.ts`).
-->
<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import AppWindow from "@lucide/svelte/icons/app-window";
  import ChevronsUpDown from "@lucide/svelte/icons/chevrons-up-down";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import House from "@lucide/svelte/icons/house";
  import Library from "@lucide/svelte/icons/library";
  import LogOut from "@lucide/svelte/icons/log-out";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Plus from "@lucide/svelte/icons/plus";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import { Icon } from "@lucide/svelte";
  import { tick } from "svelte";
  import { api, type VaultsResponse } from "../lib/api";
  import { graphHref, homeHref } from "../lib/ids";
  import { graphIcon } from "../lib/icons";
  import { session } from "../lib/state/session.svelte";
  import { settings } from "../lib/state/settings.svelte";
  import { ui } from "../lib/ui.svelte";
  import { vault, vaultHome } from "../lib/vault";
  import { openVault, vaultOpenMode, type VaultOpen } from "../lib/vault-open";

  let open = $state(false);
  let list = $state<VaultsResponse | null>(null);
  let failed = $state(false);
  let signOutError = $state<string | null>(null);
  let root: HTMLElement | undefined = $state();
  const icon = { size: 16, strokeWidth: 1.75, "aria-hidden": true } as const;

  /** The right-click menu of a vault row: where to open it. At the pointer, but within the window. */
  let choice = $state<{ name: string; x: number; y: number; from: HTMLElement } | null>(null);
  let choiceMenu: HTMLElement | undefined = $state();
  let pos = $state({ x: 0, y: 0 });

  $effect(() => {
    const at = choice;
    if (!at) return;
    pos = { x: at.x, y: at.y };
    void tick().then(() => {
      if (!choiceMenu) return;
      const box = choiceMenu.getBoundingClientRect();
      pos = { x: Math.max(4, Math.min(at.x, innerWidth - box.width - 4)), y: Math.max(4, Math.min(at.y, innerHeight - box.height - 4)) };
      choiceMenu.querySelector<HTMLElement>("[role=menuitem]")?.focus();
    });
  });

  async function toggle() {
    open = !open;
    choice = null;
    if (!open) return;
    failed = false;
    signOutError = null;
    try {
      list = await api.vaults();
    } catch {
      failed = true;
    }
  }

  async function signOut() {
    signOutError = await session.signOut();
  }

  function dialog(which: "new" | "rename" | "delete") {
    open = false;
    if (which === "new") ui.vaultNewOpen = true;
    else ui.vaultEdit = which;
  }

  function go(name: string, mode: VaultOpen) {
    open = false;
    choice = null;
    openVault(vaultHome(name), mode);
  }

  /**
   * A click on a row: in this window it is the link's own navigation (Enter
   * too), a new window is opened by hand. Shift and Alt are left to the browser.
   */
  function onClick(e: MouseEvent, name: string) {
    if (vaultOpenMode(settings.values["vaults.open"], e) === "this") return;
    e.preventDefault();
    go(name, "new");
  }

  /** The middle button gives no `click`, only `auxclick`. */
  function onAuxClick(e: MouseEvent, name: string) {
    if (e.button !== 1) return;
    e.preventDefault();
    go(name, "new");
  }

  function onContextMenu(e: MouseEvent, name: string) {
    e.preventDefault();
    const from = e.currentTarget as HTMLElement;
    // From the keyboard (the menu key) the pointer position is empty: under the row.
    const box = from.getBoundingClientRect();
    const keyboard = e.clientX === 0 && e.clientY === 0;
    choice = { name, x: keyboard ? box.left + 16 : e.clientX, y: keyboard ? box.bottom : e.clientY, from };
  }

  function onPointerDown(e: PointerEvent) {
    if (choice && !choiceMenu?.contains(e.target as Node)) choice = null;
    if (open && !root?.contains(e.target as Node)) open = false;
  }

  /** Escape closes the right-click menu first, then the vault menu. */
  function onKeydown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (choice) {
      e.stopPropagation();
      choice.from.focus();
      choice = null;
    } else if (open) {
      e.stopPropagation();
      open = false;
    }
  }
</script>

<svelte:window onpointerdown={onPointerDown} onblur={() => (choice = null)} onresize={() => (choice = null)} />

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
        <a
          role="menuitem"
          href={vaultHome(name)}
          class:current
          aria-current={current ? "true" : undefined}
          onclick={current ? undefined : (e) => onClick(e, name)}
          onauxclick={current ? undefined : (e) => onAuxClick(e, name)}
          oncontextmenu={current ? undefined : (e) => onContextMenu(e, name)}
        >
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
      {#if session.login}
        <!-- Only on a server with sign-in. -->
        <div class="vault-menu-actions">
          {#if signOutError}<p class="vault-menu-note dialog-error" role="alert">Не удалось выйти: {signOutError}</p>{/if}
          <button type="button" role="menuitem" id="sign-out" onclick={signOut}>
            <span class="vault-menu-mark"><LogOut {...icon} /></span>
            <span class="vault-menu-name" title="Выйти из аккаунта">Выйти ({session.login})</span>
          </button>
        </div>
      {/if}
    </div>
  {/if}
  {#if open && choice}
    <div
      class="note-menu"
      id="vault-open-menu"
      role="menu"
      aria-label="Где открыть хранилище «{choice.name}»"
      bind:this={choiceMenu}
      style:left="{pos.x}px"
      style:top="{pos.y}px"
    >
      <div class="note-menu-head" title="Хранилище «{choice.name}»"><Library {...icon} /><span>{choice.name}</span></div>
      <button type="button" role="menuitem" onclick={() => go(choice!.name, "this")}><AppWindow {...icon} />В этом окне</button>
      <button type="button" role="menuitem" onclick={() => go(choice!.name, "new")}><ExternalLink {...icon} />В новом окне</button>
    </div>
  {/if}
</div>
