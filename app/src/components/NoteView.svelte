<!--
  Заметка: вставка готового HTML с сервера (у книги — одной главы), прокрутка
  к якорю, переход между главами. HTML заметки большой и чужой — вставляется
  напрямую в DOM, а не шаблоном Svelte.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { app } from "../lib/app.svelte";
  import { showChapter, splitBook } from "../lib/book";
  import type { NotePage } from "../lib/api";
  import { tagHref } from "../lib/ids";
  import { ui } from "../lib/ui.svelte";
  import Loading from "./Loading.svelte";

  let content: HTMLDivElement | undefined = $state();
  let seenAnchorSeq = 0;

  $effect(() => {
    const page = app.page;
    const el = content;
    if (page && el) untrack(() => render(page, el));
  });

  // Переход по якорю в той же заметке (ссылка, оглавление, «назад»).
  $effect(() => {
    const seq = app.anchorSeq;
    if (seq === seenAnchorSeq) return;
    seenAnchorSeq = seq;
    untrack(() => {
      if (!app.page) return;
      if (scrollToAnchor(app.anchor)) return;
      // «Назад» к месту без якоря — туда, где были; иначе — к началу.
      const place = app.restore;
      app.restore = null;
      if (ui.book) setChapter(place?.chapter ?? 0);
      scrollTo(0, place?.y ?? 0);
    });
  });

  function render(page: NotePage, el: HTMLDivElement) {
    const r = page.rendered;
    const html = r ? r.styles + r.body : "";
    const intent = app.scroll;
    const book = r && app.settings["books.pages"] === "chapters" ? splitBook(html) : null;
    ui.book = book;
    if (book) {
      for (const n of book.intro) if (n instanceof Element) linkTags(n);
      el.replaceChildren(book.fragment);
      const byHash = app.anchor != null ? book.byAnchor.get(app.anchor) : undefined;
      setChapter(intent.mode === "keep" && intent.chapter != null ? intent.chapter : (byHash ?? 0));
    } else {
      el.innerHTML = html;
      linkTags(el);
      app.chapter = null;
    }
    if (intent.mode === "keep") scrollTo(0, intent.y);
    else if (intent.mode === "anchor" && scrollToAnchor(app.anchor)) holdAnchor(page.id);
    else scrollTo(0, 0);
    document.documentElement.dataset.state = "ready";
  }

  /** Теги в шапке заметки — ссылки на страницу тега. */
  function linkTags(root: Element) {
    for (const li of root.querySelectorAll(".k-tags li")) {
      const tag = li.textContent?.trim();
      if (tag) li.replaceChildren(Object.assign(document.createElement("a"), { href: tagHref(tag), textContent: tag }));
    }
  }

  function setChapter(k: number) {
    if (!ui.book) return;
    const n = Math.min(Math.max(k, 0), ui.book.chapters.length - 1);
    showChapter(ui.book, n);
    ui.chapter = n;
    app.chapter = n;
  }

  /** Прокрутить к разделу (у книги — сначала открыть его главу). */
  function scrollToAnchor(name: string | null): boolean {
    if (!name || !content) return false;
    const k = ui.book?.byAnchor.get(name);
    if (k != null && k !== ui.chapter) setChapter(k);
    const el = document.getElementById(name) ?? content.querySelector(`[data-k-anchor="${CSS.escape(name)}"]`);
    el?.scrollIntoView();
    return Boolean(el);
  }

  /**
   * Шрифты догружаются уже после вставки заметки (font-display: swap), и
   * текст выше якоря перестраивается — якорь уезжает. Пару секунд после
   * перехода возвращаемся к нему после каждой догрузки шрифтов.
   */
  function holdAnchor(id: string) {
    const anchor = app.anchor;
    const again = () => app.page?.id === id && app.anchor === anchor && scrollToAnchor(anchor);
    document.fonts.addEventListener("loadingdone", again);
    requestAnimationFrame(() => void document.fonts.ready.then(again));
    setTimeout(() => document.fonts.removeEventListener("loadingdone", again), 2000);
  }
</script>

<main class="k-note" id="note">
  {#if app.page}
    <div class="note-body" bind:this={content}></div>
  {:else if app.failure}
    <p class="welcome">{app.failure}</p>
  {:else if app.pending}
    <Loading id={app.pending.id} since={app.pending.since} />
  {/if}
</main>
