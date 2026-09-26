<!--
  Заметка: вставка готового HTML с сервера (у книги — одной главы: главы режет
  сервер), прокрутка к якорю, переход между главами. HTML заметки большой и чужой — вставляется
  напрямую в DOM, а не шаблоном Svelte.
-->
<script lang="ts">
  import { mount, onDestroy, unmount, untrack } from "svelte";
  import { app } from "../lib/app.svelte";
  import type { NotePage } from "../lib/api";
  import { tagHref } from "../lib/ids";
  import { ui } from "../lib/ui.svelte";
  import { formulasOk, readSpec } from "../lib/plot/spec";
  import Loading from "./Loading.svelte";
  import Plot from "./Plot.svelte";

  let content: HTMLDivElement | undefined = $state();
  let seenAnchorSeq = 0;
  let plots: ReturnType<typeof mount>[] = [];
  onDestroy(unmountPlots);

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
      const chapter = place?.chapter ?? 0;
      if (ui.book && chapter !== ui.chapter) app.showChapter(chapter, { mode: "keep", y: place?.y ?? 0, chapter });
      else scrollTo(0, place?.y ?? 0);
    });
  });

  function render(page: NotePage, el: HTMLDivElement) {
    const r = page.rendered;
    const intent = app.scroll;
    unmountPlots();
    el.innerHTML = r ? r.styles + r.body : "";
    linkTags(el);
    mountPlots(el);
    ui.book = page.book;
    ui.chapter = page.book?.chapter ?? 0;
    app.chapter = page.book ? page.book.chapter : null;
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

  /**
   * Интерактивные рисунки konspekt: в HTML — `div.k-plot` с JSON и кадром
   * Typst; живой рисунок встаёт рядом, кадр прячет CSS (`[data-live]`).
   * Формула не разобралась — остаётся кадр.
   */
  function mountPlots(root: Element) {
    for (const el of root.querySelectorAll<HTMLElement>(".k-plot[data-k-plot]")) {
      const spec = readSpec(el);
      if (!spec || !formulasOk(spec)) continue;
      try {
        plots.push(mount(Plot, { target: el, props: { spec } }));
        el.dataset.live = "";
      } catch (e) {
        console.warn("интерактивный рисунок:", e);
      }
    }
  }

  function unmountPlots() {
    for (const p of plots) void unmount(p);
    plots = [];
  }

  /** Прокрутить к разделу; раздел в другой главе книги — загрузить её (прокрутит сама). */
  function scrollToAnchor(name: string | null): boolean {
    if (!name || !content) return false;
    const k = ui.book?.anchors[name];
    if (k != null && k !== ui.chapter) {
      // Уже грузится (глава или сборка) — после загрузки прокрутит сама.
      if (!app.pending) app.showChapter(k, { mode: "anchor" });
      return true;
    }
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
