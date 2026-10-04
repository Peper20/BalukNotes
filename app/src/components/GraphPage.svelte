<!--
  Граф заметок во весь экран: масштаб, сдвиг, перестановка узлов, фильтры
  (папки, тег, несуществующие, без связей, книги главами), поиск по графу, «соседи
  заметки» (`/graph?around=…&depth=…`) и граф папки (`/graph?folder=…`, её
  заметки с подпапками). Фильтры запоминаются в браузере.
  Силы графа — панель сбоку (`GraphForces`), значения — настройки `graph.*`.
-->
<script lang="ts">
  import Maximize from "@lucide/svelte/icons/maximize";
  import Minus from "@lucide/svelte/icons/minus";
  import Plus from "@lucide/svelte/icons/plus";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import { api, type Forces, type GraphFilter, type GraphLayout } from "../lib/api";
  import { DRAG, type Drag } from "../lib/graph-physics";
  import { notes, router, settings } from "../lib/state";
  import { matches } from "../lib/graph-view";
  import { folderHref, graphHref, noteHref } from "../lib/ids";
  import { plural } from "../lib/plural";
  import { noteTags } from "../lib/tags";
  import { load, save } from "../lib/storage";
  import Graph from "./Graph.svelte";
  import GraphForces from "./GraphForces.svelte";
  import GraphLegend from "./GraphLegend.svelte";
  import { scroller } from "../lib/scroll";

  let { route }: { route: { around: string | null; depth: number; folder: string | null } } = $props();

  type Saved = Pick<GraphFilter, "hidden" | "tag" | "missing" | "orphans" | "chapters">;
  const saved = load<Partial<Saved>>("k-graph", {});
  let prefs = $state<Saved>({
    hidden: Array.isArray(saved.hidden) ? saved.hidden : [],
    tag: typeof saved.tag === "string" ? saved.tag : null,
    missing: saved.missing ?? true,
    orphans: saved.orphans ?? true,
    chapters: saved.chapters ?? false,
  });
  $effect(() => save("k-graph", $state.snapshot(prefs)));

  // Силы графа: настройки `graph.*`; пока ползунок двигают — его значение (`live`).
  const forceDefs = $derived(settings.schema?.settings.filter((d) => d.key.startsWith("graph.")) ?? []);
  let live = $state<Record<string, number>>({});
  const tune = $derived(
    Object.fromEntries(forceDefs.map((d) => [d.key, live[d.key] ?? Number(settings.values[d.key] ?? d.default)])) as Record<string, number>,
  );
  // Силы раскладки — строкой: граф запрашивается заново, только когда они
  // правда изменились (ползунки соседей раскладку не трогают). Схемы ещё
  // нет — по умолчанию ядра.
  const forces = $derived(
    forceDefs.length
      ? JSON.stringify({ clusters: tune["graph.clusters"]!, center: tune["graph.center"]!, repel: tune["graph.repel"]!, links: tune["graph.links"]! } satisfies Forces)
      : null,
  );
  const drag = $derived<Drag>(forceDefs.length ? { pull: tune["graph.pull"]! / 100, back: tune["graph.return"]! / 100 } : DRAG);
  /** Панель сил выдвигают кнопкой; при каждом заходе на граф она спрятана. */
  let forcesOpen = $state(false);
  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && forcesOpen && !e.defaultPrevented && !document.querySelector("dialog[open]")) forcesOpen = false;
  }
  async function saveForce(key: string, value: number) {
    await settings.save({ [key]: value });
    const { [key]: _, ...rest } = live;
    live = rest;
  }
  function resetForces() {
    live = {};
    void settings.save(Object.fromEntries(forceDefs.map((d) => [d.key, d.default as number])));
  }

  // Граф по фильтру — с сервера, уже разложенный (фильтр и раскладка — в ядре).
  let shown = $state.raw<GraphLayout | null>(null);
  let failed = $state(false);
  let first = true;
  $effect(() => {
    document.title = "Граф — Заметки";
  });
  $effect(() => {
    const filter: Partial<GraphFilter> = { ...$state.snapshot(prefs), around: route.around, depth: route.depth, folder: route.folder, ...(forces && { forces: JSON.parse(forces) as Forces }) };
    if (first) document.documentElement.dataset.state = "loading";
    let stale = false;
    api
      .graphLayout(filter)
      .then((g) => !stale && ((shown = g), (failed = false)))
      .catch(() => !stale && (failed = true))
      .finally(() => {
        if (!first || stale) return;
        first = false;
        requestAnimationFrame(() => (document.documentElement.dataset.state = "ready"));
      });
    return () => (stale = true);
  });

  const titles = $derived(new Map(notes.all.map((n) => [n.id, n.title] as const)));
  const allTags = $derived([...new Set(notes.all.flatMap(noteTags))].sort((a, b) => a.localeCompare(b, "ru")));
  // Цвета — по всем группам, а не по показанным: фильтр не перекрашивает узлы.
  const groups = $derived(shown?.groups ?? []);

  let query = $state("");
  const hits = $derived(shown && query.trim() ? new Set(shown.nodes.filter((n) => matches(query, n.id, titles.get(n.id))).map((n) => n.id)) : null);

  let graph: Graph | undefined = $state();
  /** Узлы переставлены — есть что вернуть. */
  let moved = $state(false);

  function toggleGroup(g: string) {
    prefs.hidden = prefs.hidden.includes(g) ? prefs.hidden.filter((h) => h !== g) : [...prefs.hidden, g];
  }

  function onsearchkey(e: KeyboardEvent) {
    if (e.key === "Enter" && hits?.size) graph?.show([...hits][0]!);
    if (e.key === "Escape") query = "";
  }

  // Страница — во всю высоту колонки под верхними панелями.
  let page: HTMLElement | undefined = $state();
  let top = $state(0);
  $effect(() => {
    const measure = () => {
      const s = scroller();
      top = page ? page.getBoundingClientRect().top - s.getBoundingClientRect().top + s.scrollTop : 0;
    };
    measure();
    addEventListener("resize", measure);
    return () => removeEventListener("resize", measure);
  });

</script>

<svelte:window {onkeydown} />

<main class="graph-page" id="note" bind:this={page} style:height="calc(100% - {top}px)">
  <div class="graph-bar">
    {#if route.around}
      <span class="graph-title">
        Соседи
        <a href={noteHref(route.around)} title={route.around}>{notes.title(route.around)}</a>
        <select aria-label="Глубина" value={route.depth} onchange={(e) => router.go(graphHref(route.around, Number(e.currentTarget.value)), { replace: true })}>
          {#each [1, 2, 3] as d (d)}<option value={d}>{d} {plural(d, "шаг", "шага", "шагов")}</option>{/each}
        </select>
        <a class="graph-all" href={graphHref()}>весь граф</a>
      </span>
    {:else if route.folder}
      <span class="graph-title">
        Граф папки
        <a href={folderHref(route.folder)} title={`папка ${route.folder}`}>{notes.folderTitle(route.folder)}</a>
        <a class="graph-all" href={graphHref()}>весь граф</a>
      </span>
    {:else}
      <span class="graph-title">Граф заметок</span>
    {/if}
    <input type="search" class="graph-search" placeholder="Найти на графе" aria-label="Найти на графе" bind:value={query} onkeydown={onsearchkey} />
    <select class="graph-tag" aria-label="Тег" bind:value={prefs.tag}>
      <option value={null}>все теги</option>
      {#each allTags as t (t)}<option value={t}>#{t}</option>{/each}
    </select>
    <label><input type="checkbox" bind:checked={prefs.missing} /> ненаписанные</label>
    <label><input type="checkbox" bind:checked={prefs.orphans} /> без связей</label>
    <label title="Книга — корнем и главами вокруг него"><input type="checkbox" bind:checked={prefs.chapters} /> книги главами</label>
    <span class="graph-zoom">
      <button type="button" class="icon" title="Мельче" aria-label="мельче" onclick={() => graph?.zoom(1 / 1.3)}><Minus size={18} strokeWidth={1.75} aria-hidden="true" /></button>
      <button type="button" class="icon" title="Крупнее" aria-label="крупнее" onclick={() => graph?.zoom(1.3)}><Plus size={18} strokeWidth={1.75} aria-hidden="true" /></button>
      <button type="button" class="icon" title="Вписать в окно" aria-label="вписать" onclick={() => graph?.fit()}><Maximize size={18} strokeWidth={1.75} aria-hidden="true" /></button>
      <button type="button" class="icon" title="Вернуть раскладку: узлы — на свои места" aria-label="вернуть раскладку" disabled={!moved} onclick={() => graph?.restore()}><Undo2 size={18} strokeWidth={1.75} aria-hidden="true" /></button>
      <button type="button" class="icon" class:on={forcesOpen} title={forcesOpen ? "Спрятать силы графа" : "Силы графа"} aria-label="силы графа" aria-pressed={forcesOpen} onclick={() => (forcesOpen = !forcesOpen)}><SlidersHorizontal size={18} strokeWidth={1.75} aria-hidden="true" /></button>
    </span>
    {#if shown}
      <span class="graph-count">
        {shown.nodes.length} {plural(shown.nodes.length, "узел", "узла", "узлов")} · {shown.edges.length}
        {plural(shown.edges.length, "связь", "связи", "связей")}{#if hits} · найдено {hits.size}{/if}
      </span>
    {/if}
  </div>
  {#if groups.length > 1}<GraphLegend {groups} hidden={prefs.hidden} ontoggle={toggleGroup} />{/if}
  <div class="graph-body">
    <section class="graph-canvas">
      {#if failed}
        <p class="graph-empty">Не удалось получить граф с сервера.</p>
      {:else if shown && !shown.nodes.length}
        <p class="graph-empty">Под фильтр ничего не подошло.</p>
      {:else if shown}
        <Graph
          bind:this={graph}
          bind:moved
          layout={shown}
          interactive
          {titles}
          highlight={hits}
          {drag}
          onopen={(id, background, anchor) => router.open(id, anchor, { background })}
        />
      {/if}
    </section>
    {#if forcesOpen && forceDefs.length}
      <GraphForces defs={forceDefs} values={tune} oninput={(k, v) => (live = { ...live, [k]: v })} onchange={saveForce} onreset={resetForces} onclose={() => (forcesOpen = false)} />
    {/if}
  </div>
</main>
