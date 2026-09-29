<!-- Меню заметки в дереве (правый клик, долгое касание): действия с заметкой. -->
<script lang="ts">
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import { tick } from "svelte";
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

  function remove() {
    const id = ui.noteMenu?.id ?? null;
    close();
    ui.deleting = id;
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
  <div class="note-menu" id="note-menu" role="menu" aria-label="Заметка" bind:this={menu} style:left="{pos.x}px" style:top="{pos.y}px">
    <button type="button" role="menuitem" class="danger" onclick={remove}>
      <Trash2 size={16} strokeWidth={1.75} aria-hidden="true" />Удалить…
    </button>
  </div>
{/if}
