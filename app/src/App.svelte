<!-- Каркас приложения: панель, верхняя строка, заметка или главная, настройки. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { reader, router, settings, start } from "./lib/state";
  import { commands } from "./lib/commands.svelte";
  import { isAppPath } from "./lib/ids";
  import { typing } from "./lib/keys";
  import { mobile, ui } from "./lib/ui.svelte";
  import Backlinks from "./components/Backlinks.svelte";
  import ChapterNav from "./components/ChapterNav.svelte";
  import FolderPage from "./components/FolderPage.svelte";
  import GraphPage from "./components/GraphPage.svelte";
  import Help from "./components/Help.svelte";
  import Home from "./components/Home.svelte";
  import LinkPreview from "./components/LinkPreview.svelte";
  import Palette from "./components/Palette.svelte";
  import TabBar from "./components/TabBar.svelte";
  import Tags from "./components/Tags.svelte";
  import NoteDelete from "./components/NoteDelete.svelte";
  import NoteRename from "./components/NoteRename.svelte";
  import NoteView from "./components/NoteView.svelte";
  import Problems from "./components/Problems.svelte";
  import Settings from "./components/Settings.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import Toc from "./components/Toc.svelte";
  import Topbar from "./components/Topbar.svelte";
  import VaultEdit from "./components/VaultEdit.svelte";
  import VaultNew from "./components/VaultNew.svelte";

  let started = $state(false);
  let fatal = $state<string | null>(null);

  onMount(() => {
    start()
      .then(() => (started = true))
      .catch((e: Error) => (fatal = e.message));
  });

  $effect(() => settings.apply(document.documentElement));

  // Высота верхней панели (строка + вкладки) - для отступов якорей и оглавления.
  // Дробная (borderBoxSize, а не offsetHeight): панель - fixed, под неё отступ
  // заметки, округление сдвинуло бы заметку на пиксель.
  let chromeBox: ResizeObserverSize[] | undefined = $state();
  const chromeHeight = $derived(chromeBox?.[0]?.blockSize ?? 0);
  $effect(() => document.documentElement.style.setProperty("--chrome-h", `${chromeHeight}px`));

  // Место чтения — по ходу прокрутки (к «назад» запись истории уже другая).
  let rememberTimer: ReturnType<typeof setTimeout> | undefined;
  function onScroll() {
    clearTimeout(rememberTimer);
    rememberTimer = setTimeout(() => reader.remember(), 250);
  }

  /**
   * Ссылки на заметки, теги и якоря — внутри клиента, без перезагрузки.
   * Ctrl+клик и средняя кнопка — в фоновой вкладке приложения, а не браузера.
   */
  function onClick(e: MouseEvent) {
    const a = (e.target as Element | null)?.closest?.("a[href]") as HTMLAnchorElement | null;
    if (!a || a.target || e.defaultPrevented || e.button > 1 || e.shiftKey || e.altKey) return;
    const url = new URL(a.href, location.href);
    if (url.origin !== location.origin || !isAppPath(url.pathname)) return;
    const newTab = e.button === 1 || e.ctrlKey || e.metaKey;
    if (url.pathname === location.pathname && url.search === location.search && !newTab) {
      // Якорь в той же заметке: прокрутка — клиентом, запоминаем в истории.
      if (url.hash) {
        e.preventDefault();
        router.go(url);
      }
      return;
    }
    e.preventDefault();
    if (newTab) return router.behind(url.pathname + url.search + url.hash);
    if (mobile.matches) ui.sidebarOpen = false;
    router.go(url.pathname + url.search + url.hash);
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
      if (!c.keys?.some((k) => k.test(e)) || !((c.keysAvailable ?? c.available)?.() ?? true)) continue;
      e.preventDefault();
      c.run();
      return;
    }
  }
</script>

<svelte:document onclick={onClick} onauxclick={onClick} />
<svelte:window onkeydown={onKeydown} onscroll={onScroll} />

{#if fatal}
  <p class="fatal">Не удалось запустить клиент: {fatal}. <a href="/">Выбрать хранилище</a></p>
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
      <div class="chrome" bind:borderBoxSize={chromeBox}>
        <Topbar />
        <TabBar />
      </div>
      {#if router.route.kind === "note"}
        {#if reader.page}<Problems page={reader.page} />{/if}
        <NoteView />
        <ChapterNav />
        {#if reader.page}<Backlinks id={reader.page.id} />{/if}
        <Toc />
      {:else if router.route.kind === "graph" && started}
        <GraphPage route={router.route} />
      {:else if router.route.kind === "tags" && started}
        <Tags tag={router.route.tag} />
      {:else if router.route.kind === "folder" && started}
        <FolderPage path={router.route.path} />
      {:else if started}
        <Home />
      {/if}
    </div>
  </div>
  <Settings />
  <Help />
  <VaultNew />
  <VaultEdit />
  <NoteDelete />
  <NoteRename />
  <Palette />
  <LinkPreview />
{/if}
