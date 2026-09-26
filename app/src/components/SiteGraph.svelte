<!--
  Граф заметок статического сайта (`graph.html`, часть `static-graph.js`):
  тот же граф, что на странице графа приложения, — масштаб, сдвиг,
  перестановка узлов, поиск по графу. Граф — всего хранилища, разложенный
  ядром при сборке сайта; фильтров нет (они требуют раскладки ядром).
-->
<script lang="ts">
  import Maximize from "@lucide/svelte/icons/maximize";
  import Minus from "@lucide/svelte/icons/minus";
  import Plus from "@lucide/svelte/icons/plus";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import type { GraphLayout } from "../lib/api";
  import { matches } from "../lib/graph-view";
  import type { OpenNote } from "../lib/live/block";
  import Graph from "./Graph.svelte";
  import GraphLegend from "./GraphLegend.svelte";

  let { layout, onopen }: { layout: GraphLayout; onopen: OpenNote } = $props();

  let query = $state("");
  const hits = $derived(query.trim() ? new Set(layout.nodes.filter((n) => matches(query, n.id)).map((n) => n.id)) : null);
  let graph: Graph | undefined = $state();
  let moved = $state(false);

  function onsearchkey(e: KeyboardEvent) {
    if (e.key === "Enter" && hits?.size) graph?.show([...hits][0]!);
    if (e.key === "Escape") query = "";
  }

  const plural = (n: number, one: string, few: string, many: string) => {
    const m10 = n % 10;
    const m100 = n % 100;
    return m10 === 1 && m100 !== 11 ? one : m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14) ? few : many;
  };
  const icon = { size: 18, strokeWidth: 1.75, "aria-hidden": true } as const;
</script>

<div class="k-site-graph-bar">
  <h1 class="k-site-graph-title">Граф заметок</h1>
  <input type="search" class="k-site-graph-search" placeholder="Найти на графе" aria-label="Найти на графе" bind:value={query} onkeydown={onsearchkey} />
  <span class="k-site-graph-zoom">
    <button type="button" title="Мельче" aria-label="мельче" onclick={() => graph?.zoom(1 / 1.3)}><Minus {...icon} /></button>
    <button type="button" title="Крупнее" aria-label="крупнее" onclick={() => graph?.zoom(1.3)}><Plus {...icon} /></button>
    <button type="button" title="Вписать в окно" aria-label="вписать" onclick={() => graph?.fit()}><Maximize {...icon} /></button>
    <button type="button" title="Вернуть раскладку: узлы — на свои места" aria-label="вернуть раскладку" disabled={!moved} onclick={() => graph?.restore()}
      ><Undo2 {...icon} /></button
    >
  </span>
  <span class="k-site-graph-count">
    {layout.nodes.length} {plural(layout.nodes.length, "узел", "узла", "узлов")} · {layout.edges.length}
    {plural(layout.edges.length, "связь", "связи", "связей")}{#if hits} · найдено {hits.size}{/if}
  </span>
</div>
{#if layout.groups.length > 1}<GraphLegend groups={layout.groups} />{/if}
<section class="k-site-graph-canvas">
  {#if layout.nodes.length}
    <Graph bind:this={graph} bind:moved {layout} interactive highlight={hits} {onopen} />
  {:else}
    <p class="k-site-graph-empty">Заметок пока нет.</p>
  {/if}
</section>
<p class="k-site-graph-hint">
  Колесо или два пальца — масштаб, протянуть фон — сдвиг, протянуть узел — переставить (вернуть всех на места — кнопкой ↶),
  щелчок — открыть (Ctrl — в новой вкладке).
</p>
