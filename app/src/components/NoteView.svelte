<!--
  Заметка: вставка готового HTML с сервера (у книги — одной главы: главы режет
  сервер), прокрутка к якорю, переход между главами. HTML заметки большой и чужой — вставляется
  напрямую в DOM, а не шаблоном Svelte.
-->
<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { reader, router } from "../lib/state";
  import type { NotePage } from "../lib/api";
  import { tagHref } from "../lib/ids";
  import { mountLive } from "../lib/live";
  import { ui } from "../lib/ui.svelte";
  import Loading from "./Loading.svelte";

  let content: HTMLDivElement | undefined = $state();
  let seenAnchorSeq = 0;
  /** Что сейчас показано: заметка и глава — чтобы при перерисовке той же сохранить раскрытое. */
  let shown: string | null = null;
  let unmountLive = () => {};
  onDestroy(() => unmountLive());

  $effect(() => {
    const page = reader.page;
    const el = content;
    if (page && el) untrack(() => render(page, el));
  });

  // Переход по якорю в той же заметке (ссылка, оглавление, «назад»).
  $effect(() => {
    const seq = router.anchorSeq;
    if (seq === seenAnchorSeq) return;
    seenAnchorSeq = seq;
    untrack(() => {
      if (!reader.page) return;
      if (scrollToAnchor(router.anchor)) return;
      // «Назад» к месту без якоря — туда, где были; иначе — к началу.
      const place = reader.restore;
      reader.restore = null;
      const chapter = place?.chapter ?? 0;
      if (ui.book && chapter !== ui.chapter) reader.showChapter(chapter, { mode: "keep", y: place?.y ?? 0, chapter });
      else scrollTo(0, place?.y ?? 0);
    });
  });

  function render(page: NotePage, el: HTMLDivElement) {
    const r = page.rendered;
    const intent = reader.scroll;
    unmountLive();
    // Та же заметка пересобрана (правка файла, «обновить») — раскрытые
    // «Ответы» и прочие <details> не должны свернуться сами.
    const key = `${page.id}#${page.book?.chapter ?? ""}`;
    const open = key === shown ? [...el.querySelectorAll("details")].map((d) => d.open) : [];
    shown = key;
    el.innerHTML = r ? r.styles + r.body : "";
    el.querySelectorAll("details").forEach((d, i) => open[i] && (d.open = true));
    linkTags(el);
    unmountLive = mountLive(el, (id, newTab) => router.open(id, null, { newTab }));
    ui.book = page.book;
    ui.chapter = page.book?.chapter ?? 0;
    reader.chapter = page.book ? page.book.chapter : null;
    if (intent.mode === "keep") scrollTo(0, intent.y);
    else if (intent.mode === "anchor" && scrollToAnchor(router.anchor)) holdAnchor(page.id);
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

  /** Прокрутить к разделу; раздел в другой главе книги — загрузить её (прокрутит сама). */
  function scrollToAnchor(name: string | null): boolean {
    if (!name || !content) return false;
    const k = ui.book?.anchors[name];
    if (k != null && k !== ui.chapter) {
      // Уже грузится (глава или сборка) — после загрузки прокрутит сама.
      if (!reader.pending) reader.showChapter(k, { mode: "anchor" });
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
