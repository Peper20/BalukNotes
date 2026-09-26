<!--
  Граф заметок во весь экран: масштаб, сдвиг, перестановка узлов, фильтры
  (папки, тег, несуществующие, без связей), поиск по графу и «соседи
  заметки» (`/graph?around=…&depth=…`). Фильтры запоминаются в браузере.
-->
<script lang="ts">
  import Maximize from "@lucide/svelte/icons/maximize";
  import Minus from "@lucide/svelte/icons/minus";
  import Plus from "@lucide/svelte/icons/plus";
  import { api, type GraphFilter, type GraphLayout } from "../lib/api";
  import { notes, router } from "../lib/state";
  import { matches } from "../lib/graph-view";
  import { graphHref, noteHref, splitId } from "../lib/ids";
  import { load, save } from "../lib/storage";
  import Graph from "./Graph.svelte";
  import GraphLegend from "./GraphLegend.svelte";

  let { route }: { route: { around: string | null; depth: number } } = $props();

  type Saved = Pick<GraphFilter, "hidden" | "tag" | "missing" | "orphans">;
  const saved = load<Partial<Saved>>("k-graph", {});
  let prefs = $state<Saved>({
    hidden: Array.isArray(saved.hidden) ? saved.hidden : [],
    tag: typeof saved.tag === "string" ? saved.tag : null,
    missing: saved.missing ?? true,
    orphans: saved.orphans ?? true,
  });
  $effect(() => save("k-graph", $state.snapshot(prefs)));

  // Граф по фильтру — с сервера, уже разложенный (фильтр и раскладка — в ядре).
  let shown = $state.raw<GraphLayout | null>(null);
  let failed = $state(false);
  let first = true;
  $effect(() => {
    document.title = "Граф — Заметки";
  });
  $effect(() => {
    const filter: Partial<GraphFilter> = { ...$state.snapshot(prefs), around: route.around, depth: route.depth };
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

  const titles = $derived(new Map(notes.all.flatMap((n) => (n.title ? [[n.id, n.title] as const] : []))));
  const allTags = $derived([...new Set(notes.all.flatMap((n) => n.tags))].sort((a, b) => a.localeCompare(b, "ru")));
  // Цвета — по всем группам, а не по показанным: фильтр не перекрашивает узлы.
  const groups = $derived(shown?.groups ?? []);

  let query = $state("");
  const hits = $derived(shown && query.trim() ? new Set(shown.nodes.filter((n) => matches(query, n.id, titles.get(n.id))).map((n) => n.id)) : null);

  let graph: Graph | undefined = $state();

  function toggleGroup(g: string) {
    prefs.hidden = prefs.hidden.includes(g) ? prefs.hidden.filter((h) => h !== g) : [...prefs.hidden, g];
  }

  function onsearchkey(e: KeyboardEvent) {
    if (e.key === "Enter" && hits?.size) graph?.show([...hits][0]!);
    if (e.key === "Escape") query = "";
  }

  // Страница — во всю высоту окна под верхними панелями.
  let page: HTMLElement | undefined = $state();
  let top = $state(0);
  $effect(() => {
    const measure = () => (top = page ? page.getBoundingClientRect().top + scrollY : 0);
    measure();
    addEventListener("resize", measure);
    return () => removeEventListener("resize", measure);
  });

  const centerNote = $derived(route.around ? notes.all.find((n) => n.id === route.around) : null);
  const plural = (n: number, one: string, few: string, many: string) => {
    const m10 = n % 10;
    const m100 = n % 100;
    return m10 === 1 && m100 !== 11 ? one : m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14) ? few : many;
  };
</script>

<main class="graph-page" id="note" bind:this={page} style:height="calc(100dvh - {top}px)">
  <div class="graph-bar">
    {#if route.around}
      <span class="graph-title">
        Соседи
        <a href={noteHref(route.around)} title={centerNote?.title ?? route.around}>{splitId(route.around).name}</a>
        <select aria-label="Глубина" value={route.depth} onchange={(e) => router.go(graphHref(route.around, Number(e.currentTarget.value)), { replace: true })}>
          {#each [1, 2, 3] as d (d)}<option value={d}>{d} {plural(d, "шаг", "шага", "шагов")}</option>{/each}
        </select>
        <a class="graph-all" href="/graph">весь граф</a>
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
    <span class="graph-zoom">
      <button type="button" class="icon" title="Мельче" aria-label="мельче" onclick={() => graph?.zoom(1 / 1.3)}><Minus size={18} strokeWidth={1.75} aria-hidden="true" /></button>
      <button type="button" class="icon" title="Крупнее" aria-label="крупнее" onclick={() => graph?.zoom(1.3)}><Plus size={18} strokeWidth={1.75} aria-hidden="true" /></button>
      <button type="button" class="icon" title="Вписать в окно" aria-label="вписать" onclick={() => graph?.fit()}><Maximize size={18} strokeWidth={1.75} aria-hidden="true" /></button>
    </span>
    {#if shown}
      <span class="graph-count">
        {shown.nodes.length} {plural(shown.nodes.length, "узел", "узла", "узлов")} · {shown.edges.length}
        {plural(shown.edges.length, "связь", "связи", "связей")}{#if hits} · найдено {hits.size}{/if}
      </span>
    {/if}
  </div>
  {#if groups.length > 1}<GraphLegend {groups} hidden={prefs.hidden} ontoggle={toggleGroup} />{/if}
  <section class="graph-canvas">
    {#if failed}
      <p class="graph-empty">Не удалось получить граф с сервера.</p>
    {:else if shown && !shown.nodes.length}
      <p class="graph-empty">Под фильтр ничего не подошло.</p>
    {:else if shown}
      <Graph
        bind:this={graph}
        layout={shown}
        interactive
        {titles}
        highlight={hits}
        onopen={(id, newTab) => router.open(id, null, { newTab })}
      />
    {/if}
  </section>
  <p class="graph-hint">
    Колесо или два пальца — масштаб, протянуть фон — сдвиг, протянуть узел — переставить (соседи потянутся за ним),
    щелчок — открыть (Ctrl — в новой вкладке).
  </p>
</main>
