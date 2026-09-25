<!--
  Вкладки (видны, когда их больше одной). Ссылка с Ctrl или средней кнопкой
  открывается в новой вкладке; средняя кнопка на вкладке — закрыть.
-->
<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { parseRoute, splitId } from "../lib/ids";

  function title(url: string): string {
    const route = parseRoute(new URL(url, location.origin).pathname);
    if (route.kind === "home") return "Главная";
    if (route.kind === "tags") return route.tag ? `#${route.tag}` : "Теги";
    const note = app.notes.find((n) => n.id === route.id);
    return note?.title ?? note?.name ?? splitId(route.id).name;
  }
</script>

{#if app.tabs.length > 1}
  <div class="tabbar" role="tablist" aria-label="Вкладки">
    {#each app.tabs as tab, i}
      <div
        class="tab"
        class:active={i === app.activeTab}
        role="tab"
        tabindex="0"
        aria-selected={i === app.activeTab}
        title={decodeURIComponent(tab.url)}
        onclick={() => app.switchTab(i)}
        onauxclick={(e) => e.button === 1 && app.closeTab(i)}
        onkeydown={(e) => e.key === "Enter" && app.switchTab(i)}
      >
        <span class="tab-title">{title(tab.url)}</span>
        <button
          type="button"
          class="tab-close"
          aria-label="Закрыть вкладку"
          onclick={(e) => {
            e.stopPropagation();
            app.closeTab(i);
          }}>×</button
        >
      </div>
    {/each}
    <button type="button" class="tab-new" title="Новая вкладка (Alt+T)" aria-label="Новая вкладка" onclick={() => app.go("/", { newTab: true })}>+</button>
  </div>
{/if}
