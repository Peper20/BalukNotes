<!--
  Силы графа — ползунки сбоку от графа на его странице (не всплывающее меню:
  граф виден и отвечает сразу). Подписи, пределы и значения по умолчанию —
  из схемы настроек ядра (`graph.*`), значение — в процентах от обычного.
  Раскладка (папки, отталкивание, центр, связи) пересчитывается ядром,
  перетаскивание (соседи) — физика клиента.
-->
<script lang="ts">
  import type { SettingDef } from "../lib/api";

  let {
    defs,
    values,
    oninput,
    onchange,
    onreset,
  }: {
    /** Настройки `graph.*` из схемы. */
    defs: SettingDef[];
    values: Record<string, number>;
    /** Ползунок двигают — показать сразу. */
    oninput: (key: string, value: number) => void;
    /** Отпустили — сохранить. */
    onchange: (key: string, value: number) => void;
    onreset: () => void;
  } = $props();

  const DRAG = ["graph.pull", "graph.return"];
  const sections = $derived([
    { title: "Раскладка", defs: defs.filter((d) => !DRAG.includes(d.key)) },
    { title: "Перетаскивание", defs: defs.filter((d) => DRAG.includes(d.key)) },
  ]);
  const name = (d: SettingDef) => d.label.replace(/, %$/, "");
  const changed = $derived(defs.some((d) => values[d.key] !== d.default));
</script>

<aside class="graph-forces" aria-label="Силы графа">
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
</aside>
