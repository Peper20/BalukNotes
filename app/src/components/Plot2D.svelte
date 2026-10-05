<!--
  A live plot y = f(x): SVG in the same units (pt) as the Typst frame, the
  colors are theme variables (a theme change recolors it by itself). Sliders
  change the parameters, under the pointer - the coordinates and the curve
  values.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { compile } from "../lib/plot/formula";
  import { clipRuns, formatNumber, niceStep, sample, ticks } from "../lib/plot/geometry";
  import { colorVar, defaults, PT_PER_CM, SMALL_PT, TEXT_PT, type Plot2D } from "../lib/plot/spec";
  import PlotSliders from "./PlotSliders.svelte";

  // The figure description is set once on mount (NoteView) and does not change.
  let { spec: initial }: { spec: Plot2D } = $props();
  const spec = untrack(() => initial);
  const uid = $props.id();

  // Margins around the plot area for tick and axis labels, pt.
  const M = { left: 22, right: 12, top: 12, bottom: 16 };
  const N = 400;

  const fns = spec.curves.map((c) => compile(c.f, ["x", ...spec.params.map((p) => p.name)]));
  let values = $state(defaults(spec.params));

  const [a, b] = spec.x;
  const [y0, y1] = spec.y;
  const W = spec.width * PT_PER_CM;
  const H = spec.height * PT_PER_CM;
  const sx = W / (b - a);
  const sy = H / (y1 - y0);
  const X = (x: number) => M.left + (x - a) * sx;
  const Y = (y: number) => M.top + (y1 - y) * sy;
  const ox = a <= 0 && 0 <= b ? 0 : a;
  const oy = y0 <= 0 && 0 <= y1 ? 0 : y0;
  const hx = niceStep(b - a);
  const hy = niceStep(y1 - y0);
  const xt = ticks(a, b, hx);
  const yt = ticks(y0, y1, hy);
  const small = SMALL_PT * 0.85;

  const curves = $derived(
    fns.map((f) => clipRuns(sample((x) => f({ ...values, x }), a, b, N), y0, y1)),
  );
  const paths = $derived(
    curves.map((runs) => runs.map((r) => "M" + r.map(([x, y]) => `${X(x).toFixed(2)},${Y(y).toFixed(2)}`).join("L")).join("")),
  );

  // The pointer: x in plot coordinates (null - outside the area).
  let hover: number | null = $state(null);
  const readout = $derived(
    hover === null ? null : fns.map((f) => f({ ...values, x: hover! })),
  );
  let svg: SVGSVGElement | undefined = $state();

  function move(e: PointerEvent) {
    if (!svg) return;
    const box = svg.getBoundingClientRect();
    const px = ((e.clientX - box.left) / box.width) * (W + M.left + M.right);
    const x = a + (px - M.left) / sx;
    hover = x >= a && x <= b ? x : null;
  }

  const fmtX = (v: number) => formatNumber(v, hx / 10);
  const fmtY = (v: number | null) => (v === null ? "—" : formatNumber(v, hy / 10));
</script>

<div class="k-plot-live">
  <svg
    bind:this={svg}
    class="k-plot-svg"
    viewBox="0 0 {W + M.left + M.right} {H + M.top + M.bottom}"
    style:width="{(W + M.left + M.right) / TEXT_PT}em"
    role="img"
    aria-label="график"
    onpointermove={move}
    onpointerdown={move}
    onpointerleave={() => (hover = null)}
  >
    <defs>
      <marker id="{uid}-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto">
        <path d="M0,1 L10,5 L0,9 L3,5 Z" fill="var(--k-fig-axis)" />
      </marker>
    </defs>
    <g stroke="var(--k-fig-grid)" stroke-width="0.3">
      {#each xt as x (x)}<line x1={X(x)} x2={X(x)} y1={Y(y0)} y2={Y(y1)} />{/each}
      {#each yt as y (y)}<line x1={X(a)} x2={X(b)} y1={Y(y)} y2={Y(y)} />{/each}
    </g>
    <g stroke="var(--k-fig-axis)" stroke-width="0.6">
      <line x1={X(a)} x2={X(b)} y1={Y(oy)} y2={Y(oy)} marker-end="url(#{uid}-arrow)" />
      <line x1={X(ox)} x2={X(ox)} y1={Y(y0)} y2={Y(y1)} marker-end="url(#{uid}-arrow)" />
    </g>
    <g fill="var(--k-fig-axis)" font-size={small}>
      {#each xt as x (x)}
        {#if Math.abs(x - ox) > hx / 2 && a + hx / 3 < x && x < b - hx / 3}
          <text x={X(x)} y={Y(oy) + small + 2} text-anchor="middle">{formatNumber(x, hx)}</text>
        {/if}
      {/each}
      {#each yt as y (y)}
        {#if Math.abs(y - oy) > hy / 2 && y0 + hy / 3 < y && y < y1 - hy / 3}
          <text x={X(ox) - 2} y={Y(y) + small * 0.35} text-anchor="end">{formatNumber(y, hy)}</text>
        {/if}
      {/each}
    </g>
    <g fill="var(--k-fig-axis)" font-style="italic" font-size={SMALL_PT}>
      <text x={X(b)} y={Y(oy) + SMALL_PT + 7} text-anchor="end">{spec.labels[0]}</text>
      <text x={X(ox) + 5} y={Y(y1) + SMALL_PT} text-anchor="start">{spec.labels[1]}</text>
    </g>
    <g font-size={SMALL_PT}>
    {#each spec.curves as c, i (i)}
      <path
        class="k-plot-curve"
        d={paths[i]}
        fill="none"
        stroke="var({colorVar(c.color)})"
        stroke-width="1.1"
        stroke-linejoin="round"
        stroke-dasharray={c.dashed ? "3 2" : undefined}
      />
    {/each}
    </g>
    {#if hover !== null && readout}
      <line class="k-plot-guide" x1={X(hover)} x2={X(hover)} y1={Y(y0)} y2={Y(y1)} stroke="var(--k-fig-axis)" stroke-width="0.4" stroke-dasharray="2 2" />
      {#each readout as v, i (i)}
        {#if v !== null && v >= y0 && v <= y1}
          <circle cx={X(hover)} cy={Y(v)} r="2" fill="var({colorVar(spec.curves[i]!.color)})" />
        {/if}
      {/each}
    {/if}
  </svg>
  {#if spec.curves.some((c) => c.label)}
    <div class="k-plot-legend">
      {#each spec.curves as c, i (i)}
        {#if c.label}<span class="k-plot-key" style:color="var({colorVar(c.color)})">{c.dashed ? "- - " : "— "}{c.label}</span>{/if}
      {/each}
    </div>
  {/if}
  <div class="k-plot-readout" aria-live="polite">
    {#if hover !== null && readout}
      <span>{spec.labels[0]} = {fmtX(hover)}</span>
      {#each readout as v, i (i)}
        <span style:color="var({colorVar(spec.curves[i]!.color)})">{spec.curves[i]!.label ?? spec.labels[1]} = {fmtY(v)}</span>
      {/each}
    {:else}
      <span class="k-plot-hint">наведите на график или коснитесь его</span>
    {/if}
  </div>
  <PlotSliders params={spec.params} bind:values />
</div>
