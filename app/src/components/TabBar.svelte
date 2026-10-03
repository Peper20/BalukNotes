<!--
  Вкладки (видны, когда их больше одной). Ссылка с Ctrl или средней кнопкой
  открывается в новой вкладке; средняя кнопка на вкладке — закрыть.
-->
<script lang="ts">
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";
  import { notes, router, tabs } from "../lib/state";
  import { homeHref, parseRoute } from "../lib/ids";

  function title(url: string): string {
    const u = new URL(url, location.origin);
    const route = parseRoute(u.pathname, u.search);
    if (route.kind === "home") return "Главная";
    if (route.kind === "graph")
      return route.around ? `Граф: ${notes.title(route.around)}` : route.folder ? `Граф: ${notes.folderTitle(route.folder)}` : "Граф";
    if (route.kind === "tags") return route.tag ? `#${route.tag}` : "Теги";
    if (route.kind === "folder") return notes.folderTitle(route.path);
    return notes.title(route.id);
  }

  let bar: HTMLDivElement | undefined = $state();

  // Активная вкладка — в поле зрения полосы (на телефоне вкладки не влезают).
  $effect(() => {
    void [tabs.active, tabs.list.length, notes.all]; // названия (ширина) — по списку заметок
    const tab = bar?.querySelector<HTMLElement>(".tab.active");
    if (!bar || !tab) return;
    const [b, t] = [bar.getBoundingClientRect(), tab.getBoundingClientRect()];
    if (t.left < b.left) bar.scrollLeft += t.left - b.left - 8;
    else if (t.right > b.right) bar.scrollLeft += t.right - b.right + 8;
  });
</script>

{#if tabs.list.length > 1}
  <div class="tabbar" role="tablist" aria-label="Вкладки" bind:this={bar}>
    {#each tabs.list as tab, i}
      <div
        class="tab"
        class:active={i === tabs.active}
        role="tab"
        tabindex="0"
        aria-selected={i === tabs.active}
        title={decodeURIComponent(tab.url)}
        onclick={() => router.switchTab(i)}
        onauxclick={(e) => e.button === 1 && router.closeTab(i)}
        onkeydown={(e) => e.key === "Enter" && router.switchTab(i)}
      >
        <span class="tab-title">{title(tab.url)}</span>
        <button
          type="button"
          class="tab-close"
          aria-label="Закрыть вкладку"
          onclick={(e) => {
            e.stopPropagation();
            router.closeTab(i);
          }}><X size={14} strokeWidth={2} aria-hidden="true" /></button
        >
      </div>
    {/each}
    <button type="button" class="tab-new" title="Новая вкладка (Alt+T)" aria-label="Новая вкладка" onclick={() => router.go(homeHref(), { newTab: true })}><Plus size={16} strokeWidth={2} aria-hidden="true" /></button>
  </div>
{/if}
