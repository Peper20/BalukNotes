<!--
  Меню заметки или папки в дереве (правый клик, долгое касание): сверху - для
  чего оно (указатель на краю строки мог попасть в соседнюю), ниже - действия.
-->
<script lang="ts">
  import BookIcon from "@lucide/svelte/icons/book";
  import FileText from "@lucide/svelte/icons/file-text";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import { tick } from "svelte";
  import { notes } from "../lib/state";
  import { ui } from "../lib/ui.svelte";

  let menu: HTMLElement | undefined = $state();
  /** Место меню: у указателя, но в пределах окна. */
  let pos = $state({ x: 0, y: 0 });

  $effect(() => {
    const at = ui.noteMenu;
    if (!at) return;
    pos = { x: at.x, y: at.y };
    void tick().then(() => {
      if (!menu) return;
      const box = menu.getBoundingClientRect();
      pos = { x: Math.max(4, Math.min(at.x, innerWidth - box.width - 4)), y: Math.max(4, Math.min(at.y, innerHeight - box.height - 4)) };
      menu.querySelector<HTMLElement>("[role=menuitem]")?.focus();
    });
  });

  const close = () => (ui.noteMenu = null);

  /** Для чего меню: значок как в дереве, название, путь - в подсказке. */
  const head = $derived.by(() => {
    const at = ui.noteMenu;
    if (!at) return null;
    if (at.kind === "folder") return { icon: FolderIcon, title: notes.folderTitle(at.id), hint: `папка ${at.id}` };
    const book = notes.byId(at.id)?.kind === "book";
    return { icon: book ? BookIcon : FileText, title: notes.title(at.id), hint: `${book ? "книга" : "заметка"} ${at.id}` };
  });

  function rename() {
    const at = ui.noteMenu;
    close();
    if (at) ui.renaming = { kind: at.kind, id: at.id };
  }

  function remove() {
    const at = ui.noteMenu;
    close();
    if (at) ui.deleting = { kind: at.kind, id: at.id };
  }

  function onPointerDown(e: PointerEvent) {
    if (ui.noteMenu && !menu?.contains(e.target as Node)) close();
  }

  function onKeydown(e: KeyboardEvent) {
    if (ui.noteMenu && e.key === "Escape") {
      e.stopPropagation();
      close();
    }
  }
</script>

<svelte:window onpointerdown={onPointerDown} onkeydown={onKeydown} onblur={close} onresize={close} />

{#if ui.noteMenu}
  <div class="note-menu" id="note-menu" role="menu" aria-label={head?.hint} bind:this={menu} style:left="{pos.x}px" style:top="{pos.y}px">
    {#if head}
      <div class="note-menu-head" title={`${head.title}\n${head.hint}`}>
        <head.icon size={16} strokeWidth={1.75} aria-hidden="true" /><span>{head.title}</span>
      </div>
    {/if}
    <button type="button" role="menuitem" onclick={rename}>
      <Pencil size={16} strokeWidth={1.75} aria-hidden="true" />Переименовать…
    </button>
    <button type="button" role="menuitem" class="danger" onclick={remove}>
      <Trash2 size={16} strokeWidth={1.75} aria-hidden="true" />Удалить…
    </button>
  </div>
{/if}
