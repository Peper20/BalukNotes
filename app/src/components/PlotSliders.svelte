<!-- Ползунки параметров интерактивного рисунка. -->
<script lang="ts">
  import { formatNumber } from "../lib/plot/geometry";
  import type { Param } from "../lib/plot/spec";

  let { params, values = $bindable() }: { params: Param[]; values: Record<string, number> } = $props();
</script>

{#if params.length}
  <div class="k-plot-sliders">
    {#each params as p (p.name)}
      <label class="k-plot-slider">
        <span class="k-plot-param">{p.name}</span>
        <input
          type="range"
          min={p.min}
          max={p.max}
          step={p.step}
          value={values[p.name]}
          style:--k-fill="{(((values[p.name] ?? p.value) - p.min) / (p.max - p.min || 1)) * 100}%"
          oninput={(e) => (values = { ...values, [p.name]: Number(e.currentTarget.value) })}
        />
        <output>{formatNumber(values[p.name] ?? p.value, p.step)}</output>
      </label>
    {/each}
  </div>
{/if}
