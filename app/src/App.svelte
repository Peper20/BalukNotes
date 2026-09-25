<!-- Каркас приложения: панель, верхняя строка, заметка или главная, настройки. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/app.svelte";
  import { commands } from "./lib/commands.svelte";
  import { typing } from "./lib/keys";
  import { mobile, ui } from "./lib/ui.svelte";
  import Backlinks from "./components/Backlinks.svelte";
  import ChapterNav from "./components/ChapterNav.svelte";
  import Help from "./components/Help.svelte";
  import Home from "./components/Home.svelte";
  import LinkPreview from "./components/LinkPreview.svelte";
  import Palette from "./components/Palette.svelte";
  import TabBar from "./components/TabBar.svelte";
  import Tags from "./components/Tags.svelte";
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

  // Место чтения — по ходу прокрутки (к «назад» запись истории уже другая).
  let rememberTimer: ReturnType<typeof setTimeout> | undefined;
  function onScroll() {
    clearTimeout(rememberTimer);
    rememberTimer = setTimeout(() => app.remember(), 250);
  }

  /**
   * Ссылки на заметки, теги и якоря — внутри клиента, без перезагрузки.
   * Ctrl+клик и средняя кнопка — во вкладке приложения, а не браузера.
   */
  function onClick(e: MouseEvent) {
    const a = (e.target as Element | null)?.closest?.("a[href]") as HTMLAnchorElement | null;
    if (!a || a.target || e.defaultPrevented || e.button > 1 || e.shiftKey || e.altKey) return;
    const url = new URL(a.href, location.href);
    const internal = url.pathname === "/" || url.pathname.startsWith("/n/") || url.pathname.startsWith("/tags");
    if (url.origin !== location.origin || !internal) return;
    const newTab = e.button === 1 || e.ctrlKey || e.metaKey;
    if (url.pathname === location.pathname && !newTab) {
      // Якорь в той же заметке: прокрутка — клиентом, запоминаем в истории.
      if (url.hash) {
        e.preventDefault();
        app.go(url);
      }
      return;
    }
    e.preventDefault();
    if (mobile.matches) ui.sidebarOpen = false;
    app.go(url.pathname + url.hash, { newTab });
  }

  /** Горячие клавиши — по реестру команд; одиночные — не во время ввода. */
  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (ui.tocOpen) ui.tocOpen = false;
      else if (ui.reading) ui.reading = false;
      return;
    }
    if (ui.palette || e.defaultPrevented || e.repeat) return;
    const plain = !e.ctrlKey && !e.metaKey && !e.altKey;
    if (plain && typing(e)) return;
    if (document.querySelector("dialog[open]") && plain) return;
    for (const c of commands()) {
      if (!c.keys?.some((k) => k.test(e)) || !(c.available?.() ?? true)) continue;
      e.preventDefault();
      c.run();
      return;
    }
  }
</script>

<svelte:document onclick={onClick} onauxclick={onClick} />
<svelte:window onkeydown={onKeydown} onscroll={onScroll} />

{#if fatal}
  <p class="fatal">Не удалось запустить клиент: {fatal}</p>
{:else}
  <div
    class="app"
    id="app"
    class:sidebar-open={ui.sidebarOpen}
    class:sidebar-hidden={ui.sidebarHidden}
    class:toc-room={ui.tocRoom}
    class:reading={ui.reading}
  >
    <Sidebar />
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="backdrop" onclick={() => (ui.sidebarOpen = false)}></div>
    <div class="main">
      <Topbar />
      <TabBar />
      {#if app.route.kind === "note"}
        {#if app.page}<Problems page={app.page} />{/if}
        <NoteView />
        <ChapterNav />
        {#if app.page}<Backlinks id={app.page.id} />{/if}
        <Toc />
      {:else if app.route.kind === "tags" && started}
        <Tags tag={app.route.tag} />
      {:else if started}
        <Home />
      {/if}
    </div>
  </div>
  <Settings />
  <Help />
  <Palette />
  <LinkPreview />
{/if}
