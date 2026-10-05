<!--
  Graph forces: a sliding panel beside the graph on its page (not a popup
  menu: the graph is visible and responds at once; on a narrow screen - under
  the graph). Slides in and out smoothly, without a bounce. Labels, limits and
  defaults come from the core settings schema (`graph.*`), the value is in
  percent of the normal one. The layout (folders, repulsion, center, links)
  is recomputed by the core, dragging (neighbours) is the client's physics.
-->
<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import { cubicOut } from "svelte/easing";
  import { slide } from "svelte/transition";
  import type { SettingDef } from "../lib/api";

  let {
    defs,
    values,
    oninput,
    onchange,
    onreset,
    onclose,
  }: {
    /** The `graph.*` settings from the schema. */
    defs: SettingDef[];
    values: Record<string, number>;
    /** A slider moves: show at once. */
    oninput: (key: string, value: number) => void;
    /** Released: save. */
    onchange: (key: string, value: number) => void;
    onreset: () => void;
    onclose: () => void;
  } = $props();

  const reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  /** Beside: by width; under the graph (a narrow screen): by height. */
  const axis = () => (matchMedia("(max-width: 700px)").matches ? "y" : "x");

  const DRAG = ["graph.pull", "graph.return"];
  const sections = $derived([
    { title: "Раскладка", defs: defs.filter((d) => !DRAG.includes(d.key)) },
    { title: "Перетаскивание", defs: defs.filter((d) => DRAG.includes(d.key)) },
  ]);
  const name = (d: SettingDef) => d.label.replace(/, %$/, "");
  const changed = $derived(defs.some((d) => values[d.key] !== d.default));
</script>

<aside class="graph-forces" aria-label="Силы графа" transition:slide={{ axis: axis(), duration: reduced ? 0 : 220, easing: cubicOut }}>
  <div class="graph-forces-in">
    <div class="graph-forces-head">
      <h2>Силы графа</h2>
      <button type="button" class="icon" title="Спрятать (Esc)" aria-label="спрятать силы графа" onclick={onclose}><X size={16} strokeWidth={1.75} aria-hidden="true" /></button>
    </div>
    {#each sections as s (s.title)}
      <h3>{s.title}</h3>
      {#each s.defs as d (d.key)}
        {#if d.type === "number"}
          <label class="force" title={d.help}>
            <span class="force-name">{name(d)}</span>
            <output>{values[d.key]} %</output>
            <input
              type="range"
              min={d.min}
              max={d.max}
              step={d.step}
              value={values[d.key]}
              data-key={d.key}
              oninput={(e) => oninput(d.key, Number(e.currentTarget.value))}
              onchange={(e) => onchange(d.key, Number(e.currentTarget.value))}
            />
          </label>
        {/if}
      {/each}
    {/each}
    <button type="button" class="force-reset" disabled={!changed} onclick={onreset}>По умолчанию</button>
  </div>
</aside>
