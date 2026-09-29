<!--
  Превью ссылки на заметку при наведении (задержка — чтобы не мигать при
  случайном проходе мышью): название, раздел, начало текста, теги. Текст —
  из индекса исходников, без сборки заметки: быстро даже для большой книги.
-->
<script lang="ts">
  import { api, type Preview } from "../lib/api";
  import { router } from "../lib/state";
  import { hashAnchor, isAppPath, parseRoute } from "../lib/ids";

  /** Ссылки на заметки: в HTML заметки и в «Ссылаются сюда» (адреса хранилища). */
  const NOTE_LINKS = [".k-note", ".backlinks"].flatMap((c) => [`${c} a[href^='/v/']`, `${c} a[href^='/n/']`]).join(", ");

  const DELAY = 350;
  const cache = new Map<string, Preview | null>();

  let preview = $state.raw<Preview | null>(null);
  let pos = $state({ x: 0, y: 0, above: false });
  let card: HTMLElement | undefined = $state();
  let link: HTMLAnchorElement | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  function target(a: HTMLAnchorElement) {
    const url = new URL(a.href, location.href);
    if (!isAppPath(url.pathname)) return null;
    const route = parseRoute(url.pathname);
    return route.kind === "note" ? { id: route.id, anchor: hashAnchor(url.hash) } : null;
  }

  function onOver(e: PointerEvent) {
    if (e.pointerType === "touch") return;
    const a = (e.target as Element).closest?.<HTMLAnchorElement>(NOTE_LINKS);
    if ((a != null && a === link) || card?.contains(e.target as Node)) {
      clearTimeout(hideTimer);
      return;
    }
    if (!a) return;
    link = a;
    clearTimeout(timer);
    clearTimeout(hideTimer);
    const t = target(a);
    if (!t) return;
    timer = setTimeout(async () => {
      const key = `${t.id}#${t.anchor ?? ""}`;
      if (!cache.has(key)) cache.set(key, await api.preview(t.id, t.anchor).catch(() => null));
      if (link !== a) return;
      const p = cache.get(key) ?? null;
      if (!p) return;
      const r = a.getBoundingClientRect();
      const above = r.bottom + 240 > innerHeight && r.top > 260;
      pos = { x: Math.min(Math.max(r.left, 8), innerWidth - 400), y: above ? innerHeight - r.top + 6 : r.bottom + 6, above };
      preview = p;
    }, DELAY);
  }

  /** Ушли и со ссылки, и с карточки — спрятать (с задержкой: можно перейти на карточку). */
  function onOut(e: PointerEvent) {
    const ours = (n: Node | null) => n != null && Boolean(link?.contains(n) || card?.contains(n));
    if (!ours(e.target as Node) || ours(e.relatedTarget as Node | null)) return;
    clearTimeout(timer);
    hideTimer = setTimeout(() => {
      preview = null;
      link = null;
    }, 150);
  }

  // Переход — превью больше не к месту.
  $effect(() => {
    void router.route;
    void router.anchorSeq;
    preview = null;
    link = null;
  });
</script>

<svelte:document onpointerover={onOver} onpointerout={onOut} onkeydown={(e) => e.key === "Escape" && (preview = null)} />

{#if preview}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <aside
    class="link-preview"
    bind:this={card}
    style:left="{pos.x}px"
    style:top={pos.above ? "auto" : `${pos.y}px`}
    style:bottom={pos.above ? `${pos.y}px` : "auto"}
    onpointerenter={() => clearTimeout(hideTimer)}
  >
    <div class="lp-title">
      {preview.title}{#if preview.heading}<span class="lp-heading">{` › ${preview.heading}`}</span>{/if}
      {#if preview.kind === "book"}<span class="p-badge">книга</span>{/if}
    </div>
    {#if preview.text}<p class="lp-text">{preview.text}</p>{/if}
    {#if preview.tags.length}<div class="lp-tags">{#each preview.tags as t}<span>#{t}</span>{/each}</div>{/if}
  </aside>
{/if}
