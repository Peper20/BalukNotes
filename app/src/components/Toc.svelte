<!--
  Оглавление заметки (пункты — reader.toc). Сбоку от
  колонки, если справа хватает места (ширина колонки зависит от кегля),
  иначе — всплывающее по §. Подсвечивает раздел, который сейчас читают.
-->
<script lang="ts">
  import { reader, settings } from "../lib/state";
  import { ui } from "../lib/ui.svelte";

  let nav: HTMLElement | undefined = $state();

  const shown = $derived(reader.toc);

  /** Раздел, который сейчас читают: последний заголовок выше верха окна. */
  function markCurrent() {
    const visible = shown.map((h) => document.getElementById(h.id)).filter((el): el is HTMLElement => el?.isConnected ?? false);
    let current: HTMLElement | undefined;
    // Докрутили до конца — последние разделы до верха окна не доедут.
    if (innerHeight + scrollY >= document.documentElement.scrollHeight - 2) current = visible.at(-1);
    else {
      for (const el of visible) {
        if (el.getBoundingClientRect().top > 90) break;
        current = el;
      }
    }
    ui.currentHeading = (current ?? visible[0])?.id ?? null;
  }

  /** Хватает ли места справа от колонки заметки для оглавления. */
  function layout() {
    const note = document.getElementById("note");
    if (!note) return;
    const room = innerWidth - note.getBoundingClientRect().right;
    ui.tocRoom = room >= 200;
    if (ui.tocRoom) ui.tocWidth = Math.min(room - 32, 300);
    if (ui.tocRoom && settings.values["panels.toc"]) ui.tocOpen = false;
  }

  // Пересчёт: новая заметка, глава, кегль, ширина окна или панели.
  $effect(() => {
    void [shown, ui.chapter, settings.values];
    queueMicrotask(() => {
      layout();
      markCurrent();
    });
  });

  $effect(() => {
    let frame = 0;
    const onScroll = () => {
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        markCurrent();
      });
    };
    addEventListener("scroll", onScroll, { passive: true });
    const main = document.querySelector(".main");
    const ro = new ResizeObserver(layout);
    if (main) ro.observe(main);
    return () => {
      removeEventListener("scroll", onScroll);
      ro.disconnect();
    };
  });

  // Текущий пункт — в поле зрения боковой панели.
  $effect(() => {
    const id = ui.currentHeading;
    if (!id || !nav || !(ui.tocRoom || ui.tocOpen)) return;
    nav.querySelector(`a[data-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "nearest" });
  });

  function onClick(e: MouseEvent) {
    if ((e.target as Element).closest("a") && !ui.tocRoom) ui.tocOpen = false;
  }
</script>

{#if shown.length}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <nav class="toc" class:open={ui.tocOpen} aria-label="Оглавление" style:--toc-w="{ui.tocWidth}px" bind:this={nav} onclick={onClick}>
    <div class="toc-title">Содержание</div>
    {#each shown as h (h.id)}
      <a href="#{encodeURIComponent(h.id)}" data-id={h.id} data-depth={h.depth} class:current={ui.currentHeading === h.id}>{h.text}</a>
    {/each}
  </nav>
{/if}
