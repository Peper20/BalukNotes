<!-- Легенда графа: цвет — папка верхнего уровня. С `ontoggle` пункты — переключатели показа папки. -->
<script lang="ts">
  import { groupColor } from "../lib/graph-view";
  import { notes } from "../lib/state";

  let { groups, hidden = [], ontoggle }: { groups: string[]; hidden?: string[]; ontoggle?: (group: string) => void } = $props();
</script>

<div class="graph-legend">
  {#each groups as g (g)}
    {#if ontoggle}
      <button type="button" class:off={hidden.includes(g)} aria-pressed={!hidden.includes(g)} onclick={() => ontoggle(g)}>
        <i style:background={groupColor(g, groups)}></i>{notes.folderTitle(g)}
      </button>
    {:else}
      <span><i style:background={groupColor(g, groups)}></i>{notes.folderTitle(g)}</span>
    {/if}
  {/each}
</div>
