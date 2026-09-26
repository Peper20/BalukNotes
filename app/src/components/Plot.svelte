<!-- Интерактивный рисунок konspekt (`div.k-plot`): график или поверхность. -->
<script lang="ts">
  import type { PlotSpec } from "../lib/plot/spec";
  import Plot2D from "./Plot2D.svelte";
  import Plot3D from "./Plot3D.svelte";

  let { spec }: { spec: PlotSpec } = $props();
</script>

{#if spec.kind === "2d"}
  <Plot2D {spec} />
{:else}
  <Plot3D {spec} />
{/if}
