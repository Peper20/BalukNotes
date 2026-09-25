<!-- Каркас приложения: панель, верхняя строка, заметка или главная, настройки. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/app.svelte";
  import { mobile, ui } from "./lib/ui.svelte";
  import Backlinks from "./components/Backlinks.svelte";
  import ChapterNav from "./components/ChapterNav.svelte";
  import Home from "./components/Home.svelte";
  import NoteView from "./components/NoteView.svelte";
  import Problems from "./components/Problems.svelte";
  import Settings from "./components/Settings.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import Toc from "./components/Toc.svelte";
  import Topbar from "./components/Topbar.svelte";

  let started = $state(false);
  let fatal = $state<string | null>(null);

  onMount(() => {
    app
      .init()
      .then(() => (started = true))
      .catch((e: Error) => (fatal = e.message));
  });

  $effect(() => app.applyAppearance(document.documentElement));

  /** Ссылки на заметки и якоря — внутри клиента, без перезагрузки страницы. */
  function onClick(e: MouseEvent) {
    const a = (e.target as Element | null)?.closest?.("a[href]") as HTMLAnchorElement | null;
    if (!a || a.target || e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    const url = new URL(a.href, location.href);
    if (url.origin !== location.origin) return;
    if (url.pathname === location.pathname) {
      // Якорь в той же заметке: прокрутка — клиентом, запоминаем в истории.
      if (url.hash) {
        e.preventDefault();
        app.go(url);
      }
      return;
    }
    if (url.pathname === "/" || url.pathname.startsWith("/n/")) {
      e.preventDefault();
      if (mobile.matches) ui.sidebarOpen = false;
      app.go(url);
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") ui.tocOpen = false;
    const typing = (e.target as Element | null)?.closest?.("input, select, textarea, [contenteditable]");
    if (e.key === "r" && !e.ctrlKey && !e.metaKey && !e.altKey && !typing) void app.check({ force: true });
  }
</script>

<svelte:document onclick={onClick} />
<svelte:window onkeydown={onKeydown} />

{#if fatal}
  <p class="fatal">Не удалось запустить клиент: {fatal}</p>
{:else}
  <div class="app" id="app" class:sidebar-open={ui.sidebarOpen} class:sidebar-hidden={ui.sidebarHidden} class:toc-room={ui.tocRoom}>
    <Sidebar />
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="backdrop" onclick={() => (ui.sidebarOpen = false)}></div>
    <div class="main">
      <Topbar />
      {#if app.route.kind === "note"}
        {#if app.page}<Problems page={app.page} />{/if}
        <NoteView />
        <ChapterNav />
        {#if app.page}<Backlinks id={app.page.id} />{/if}
        <Toc />
      {:else if started}
        <Home />
      {/if}
    </div>
  </div>
  <Settings />
{/if}
