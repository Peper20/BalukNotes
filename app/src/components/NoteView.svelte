<!--
  A note: inserts the ready HTML from the server (of a book - one chapter:
  the server cuts the chapters), scrolls to the anchor, moves between
  chapters. The note HTML is big and foreign, so it goes straight into the
  DOM, not through a Svelte template.
-->
<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { reader, router } from "../lib/state";
  import type { NotePage } from "../lib/api";
  import { openDetails, restoreDetails } from "../lib/details";
  import { tagHref } from "../lib/ids";
  import { mountLive } from "../lib/live";
  import { ui } from "../lib/ui.svelte";
  import { inVault } from "../lib/vault";
  import Loading from "./Loading.svelte";
  import { scroller, scrollToTop } from "../lib/scroll";

  let content: HTMLDivElement | undefined = $state();
  let seenAnchorSeq = 0;
  /** What is shown now: the note and the chapter, to keep what is open when redrawing the same one. */
  let shown: string | null = null;
  let unmountLive = () => {};
  onDestroy(() => unmountLive());

  $effect(() => {
    const page = reader.page;
    const el = content;
    if (page && el) untrack(() => render(page, el));
  });

  // An anchor navigation within the same note (a link, the outline, "back").
  $effect(() => {
    const seq = router.anchorSeq;
    if (seq === seenAnchorSeq) return;
    seenAnchorSeq = seq;
    untrack(() => {
      if (!reader.page) return;
      if (scrollToAnchor(router.anchor)) return;
      // "Back" to a place without an anchor goes where we were; otherwise to the start.
      const place = reader.restore;
      reader.restore = null;
      const chapter = place?.chapter ?? 0;
      if (ui.book && chapter !== ui.chapter) reader.showChapter(chapter, { mode: "keep", y: place?.y ?? 0, chapter });
      else scrollToTop(place?.y ?? 0);
    });
  });

  function render(page: NotePage, el: HTMLDivElement) {
    const r = page.rendered;
    const intent = reader.scroll;
    unmountLive();
    // The same note was rebuilt (a file edit, "refresh"): the open "Ответы"
    // and other <details> must not collapse by themselves (by section and
    // summary text - lib/details.ts).
    const key = `${page.id}#${page.book?.chapter ?? ""}`;
    const open = key === shown ? openDetails(el) : new Set<string>();
    shown = key;
    el.innerHTML = r ? r.styles + r.body : "";
    restoreDetails(el, open);
    linkTags(el);
    linkVault(el);
    unmountLive = mountLive(el, (id, background) => router.open(id, null, { background }));
    ui.book = page.book;
    ui.chapter = page.book?.chapter ?? 0;
    reader.chapter = page.book ? page.book.chapter : null;
    if (intent.mode === "keep") scrollToTop(intent.y);
    else if (intent.mode === "anchor" && scrollToAnchor(router.anchor)) holdAnchor(page.id);
    else scrollToTop(0);
    // Scroll keys (space, PgDn) go to the note column unless the focus is in an input.
    if (!document.activeElement?.closest("input, select, textarea, [contenteditable], dialog")) scroller().focus({ preventScroll: true });
    document.documentElement.dataset.state = "ready";
  }

  /** Tags in the note header are links to the tag page. */
  function linkTags(root: Element) {
    for (const li of root.querySelectorAll(".k-tags li")) {
      const tag = li.textContent?.trim();
      if (tag) li.replaceChildren(Object.assign(document.createElement("a"), { href: tagHref(tag), textContent: tag }));
    }
  }

  /**
   * The core puts note links without a vault (`/n/...`: one HTML for all
   * addresses); they go to the shown vault, so "open in a new tab" and the
   * link address lead where a click does.
   */
  function linkVault(root: Element) {
    for (const a of root.querySelectorAll<HTMLAnchorElement>('a[href^="/n/"]')) a.setAttribute("href", inVault(a.getAttribute("href")!));
  }

  /** Scrolls to a section; a section in another book chapter - loads it (it scrolls itself). */
  function scrollToAnchor(name: string | null): boolean {
    if (!name || !content) return false;
    const k = ui.book?.anchors[name];
    if (k != null && k !== ui.chapter) {
      // Already loading (a chapter or a build): it scrolls itself after loading.
      if (!reader.pending) reader.showChapter(k, { mode: "anchor" });
      return true;
    }
    const el = document.getElementById(name) ?? content.querySelector(`[data-k-anchor="${CSS.escape(name)}"]`);
    el?.scrollIntoView();
    return Boolean(el);
  }

  /**
   * Fonts load after the note is inserted (font-display: swap), and the text
   * above the anchor reflows, so the anchor drifts. For a couple of seconds
   * after a navigation we return to it after every font load.
   */
  function holdAnchor(id: string) {
    const anchor = router.anchor;
    const again = () => reader.page?.id === id && router.anchor === anchor && scrollToAnchor(anchor);
    document.fonts.addEventListener("loadingdone", again);
    requestAnimationFrame(() => void document.fonts.ready.then(again));
    setTimeout(() => document.fonts.removeEventListener("loadingdone", again), 2000);
  }
</script>

<main class="k-note" id="note">
  {#if reader.page}
    <div class="note-body" bind:this={content}></div>
  {:else if reader.failure}
    <p class="welcome">{reader.failure}</p>
  {:else if reader.pending}
    <Loading id={reader.pending.id} since={reader.pending.since} />
  {/if}
</main>
