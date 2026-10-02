<!--
  Силы графа — выдвижная панель сбоку от графа на его странице (не
  всплывающее меню: граф виден и отвечает сразу; на узком экране — под
  графом). Выезжает и уезжает плавно, без отскока. Подписи, пределы и
  значения по умолчанию — из схемы настроек ядра (`graph.*`), значение — в
  процентах от обычного.
  Раскладка (папки, отталкивание, центр, связи) пересчитывается ядром,
  перетаскивание (соседи) — физика клиента.
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
    /** Настройки `graph.*` из схемы. */
    defs: SettingDef[];
    values: Record<string, number>;
    /** Ползунок двигают — показать сразу. */
    oninput: (key: string, value: number) => void;
    /** Отпустили — сохранить. */
    onchange: (key: string, value: number) => void;
    onreset: () => void;
    onclose: () => void;
  } = $props();

  const reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  /** Сбоку — по ширине, под графом (узкий экран) — по высоте. */
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
