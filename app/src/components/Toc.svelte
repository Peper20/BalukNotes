<!--
  The note outline (items - reader.toc; a heading with a formula is its HTML
  from the core, `Heading.html`, as trusted as the note body). Beside the
  column if there is room on the right (the column width depends on the font
  size), otherwise a popup by §. Highlights the section being read.
-->
<script lang="ts">
  import { reader, settings } from "../lib/state";
  import { ui } from "../lib/ui.svelte";
  import BookInfo from "./BookInfo.svelte";
  import { atBottom, scroller } from "../lib/scroll";

  let nav: HTMLElement | undefined = $state();

  const shown = $derived(reader.toc);

  /** The section being read: the last heading above the window top. */
  function markCurrent() {
    const visible = shown.map((h) => document.getElementById(h.id)).filter((el): el is HTMLElement => el?.isConnected ?? false);
    let current: HTMLElement | undefined;
    // Scrolled to the end: the last sections will not reach the window top.
    if (atBottom()) current = visible.at(-1);
    else {
      const top = (document.querySelector(".chrome")?.getBoundingClientRect().bottom ?? 50) + 40;
      for (const el of visible) {
        if (el.getBoundingClientRect().top > top) break;
        current = el;
      }
    }
    ui.currentHeading = (current ?? visible[0])?.id ?? null;
  }

  /** Whether there is room for the outline to the right of the note column. */
  function layout() {
    const note = document.getElementById("note");
    if (!note) return;
    const room = innerWidth - note.getBoundingClientRect().right;
    ui.tocRoom = room >= 200;
    if (ui.tocRoom) ui.tocWidth = Math.min(room - 32, 300);
    if (ui.tocRoom && settings.values["panels.toc"]) ui.tocOpen = false;
  }

  // Recompute: a new note, chapter, font size, window or panel width.
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
    const page = scroller();
    page.addEventListener("scroll", onScroll, { passive: true });
    const main = document.querySelector(".main");
    const ro = new ResizeObserver(layout);
    if (main) ro.observe(main);
    return () => {
      page.removeEventListener("scroll", onScroll);
      ro.disconnect();
    };
  });

  // The current item stays in view of the side panel.
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
    <BookInfo />
    <div class="toc-title">Содержание</div>
    {#each shown as h (h.id)}
      <a href="#{encodeURIComponent(h.id)}" data-id={h.id} data-depth={h.depth} class:current={ui.currentHeading === h.id}
        >{#if h.html}{@html h.html}{:else}{h.text}{/if}</a
      >
    {/each}
  </nav>
{/if}
