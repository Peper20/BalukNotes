<!--
  Граф заметок: узлы — заметки и книги, рёбра — ссылки #см. Цвет узла — по
  папке верхнего уровня, цвета — переменные темы: граф перекрашивается вместе
  с темой. Наведение подсвечивает узел и его соседей.
-->
<script lang="ts">
  import type { Graph } from "../lib/api";
  import { layout } from "../lib/graph-layout";
  import { splitId } from "../lib/ids";

  let { graph, onopen }: { graph: Graph; onopen: (id: string) => void } = $props();

  const ROOT = "в корне";
  const PALETTE = ["--k-accent", "--k-box-example", "--k-second", "--k-box-idea", "--k-box-algo", "--k-box-pitfall", "--k-box-thm"];

  const pos = $derived(layout(graph.nodes.map((n) => n.id), graph.edges));
  const degree = $derived.by(() => {
    const d = new Map<string, number>(graph.nodes.map((n) => [n.id, 0]));
    for (const e of graph.edges) {
      d.set(e.from, (d.get(e.from) ?? 0) + 1);
      d.set(e.to, (d.get(e.to) ?? 0) + 1);
    }
    return d;
  });
  const neighbours = $derived.by(() => {
    const m = new Map<string, Set<string>>(graph.nodes.map((n) => [n.id, new Set([n.id])]));
    for (const e of graph.edges) {
      m.get(e.from)?.add(e.to);
      m.get(e.to)?.add(e.from);
    }
    return m;
  });
  /** Группа узла — папка верхнего уровня; заметки в корне — одна группа. */
  const group = (id: string) => (id.includes("/") ? id.slice(0, id.indexOf("/")) : ROOT);
  const folders = $derived([...new Set(graph.nodes.map((n) => group(n.id)))]);
  const color = (id: string) => `var(${PALETTE[folders.indexOf(group(id)) % PALETTE.length]})`;
  const radius = (id: string, book: boolean) => (book ? 11 : 5.5) + Math.min(degree.get(id) ?? 0, 8) * 0.7;

  // По горизонтали — место под подписи (до ~20 знаков), по вертикали — под нижнюю подпись.
  const viewBox = $derived.by(() => {
    const xs = [...pos.values()].map((p) => p[0]);
    const ys = [...pos.values()].map((p) => p[1]);
    const [padX, padY] = [120, 40];
    const [x0, y0] = [Math.min(...xs) - padX, Math.min(...ys) - padY];
    return `${x0} ${y0} ${Math.max(...xs) - x0 + padX} ${Math.max(...ys) - y0 + padY + 15}`;
  });

  // Рисунок ужимается по ширине (телефон) — подписи не мельче 9 px на экране.
  let width = $state(0);
  const label = $derived.by(() => {
    const scale = width / Number(viewBox.split(" ")[2]);
    return scale > 0 ? Math.max(11, 9 / scale) : 11;
  });

  let focused = $state<string | null>(null);
  const near = $derived(focused ? neighbours.get(focused) : null);
</script>

{#if graph.nodes.length}
  <svg class="graph-svg" class:focused {viewBox} role="img" aria-label="Граф заметок" bind:clientWidth={width} style:--label="{label}px">
    {#each graph.edges as e}
      {@const a = pos.get(e.from)!}
      {@const b = pos.get(e.to)!}
      <line
        x1={a[0]} y1={a[1]} x2={b[0]} y2={b[1]}
        class="graph-edge"
        class:near={focused != null && (e.from === focused || e.to === focused)}
        stroke-width={Math.min(1 + e.count * 0.4, 3)}
      />
    {/each}
    {#each graph.nodes as n (n.id)}
      {@const [x, y] = pos.get(n.id)!}
      {@const r = radius(n.id, n.kind === "book")}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <g
        class="graph-node"
        class:missing={!n.kind}
        class:book={n.kind === "book"}
        class:near={near?.has(n.id)}
        transform="translate({x} {y})"
        data-id={n.id}
        onclick={() => n.kind && onopen(n.id)}
        onpointerenter={() => (focused = n.id)}
        onpointerleave={() => (focused = null)}
      >
        <circle {r} style:fill={n.kind ? color(n.id) : "none"} style:stroke={color(n.id)} />
        <text y={r + label + 2} text-anchor="middle">{splitId(n.id).name}</text>
        <title>
          {n.kind ? `${n.id}${n.kind === "book" ? " (книга)" : ""} · связей: ${degree.get(n.id)}` : `${n.id} — такой заметки нет`}
        </title>
      </g>
    {/each}
  </svg>
  <div class="graph-legend">
    {#each folders as f}<span><i style:background={`var(${PALETTE[folders.indexOf(f) % PALETTE.length]})`}></i>{f}</span>{/each}
  </div>
{/if}
