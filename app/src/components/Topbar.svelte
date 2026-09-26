<!-- Верхняя строка: панель, путь заметки, статус и кнопки. -->
<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { splitId } from "../lib/ids";
  import { commands, toggleToc } from "../lib/commands.svelte";
  import { ui } from "../lib/ui.svelte";

  const run = (id: string) => commands().find((c) => c.id === id)?.run();

  const crumbs = $derived(app.currentId ? splitId(app.currentId) : null);
  const themeName = $derived(app.themes.find((t) => t.name === app.theme)?.title ?? app.theme);
  const themeTitle = $derived(
    `Тема: ${app.settings["appearance.theme"] === "auto" ? `как в системе (${themeName})` : themeName}`,
  );

  // PDF собирается секунды — открываем в новой вкладке, браузер покажет его сам.
  function openPdf() {
    const url = app.pdfUrl();
    if (url) open(url, "_blank");
  }
</script>

<header class="topbar">
  <button type="button" class="icon" title="Список заметок" aria-label="Список заметок" onclick={() => run("sidebar")}>☰</button>
  <div class="crumbs" id="crumbs">
    {#if crumbs}{crumbs.folder ? `${crumbs.folder} / ` : ""}<b>{app.currentNote?.name ?? crumbs.name}</b>{/if}
  </div>
  <span class="status" class:busy={app.busy} id="status" role="status">{app.status}</span>
  <button type="button" id="open-palette" class="icon" title="Быстрый переход и поиск (Ctrl+O)" aria-label="Поиск" onclick={() => ui.openPalette("")}>⌕</button>
  {#if app.toc.length}
    <button type="button" id="toggle-toc" class="icon" title="Оглавление" aria-label="Оглавление" onclick={toggleToc}>§</button>
  {/if}
  {#if app.page}
    <button type="button" id="pdf" class="icon" title="PDF в текущей теме" aria-label="PDF" onclick={openPdf}>⎙</button>
  {/if}
  <button type="button" id="refresh" class="icon" title="Обновить (R)" aria-label="Обновить" onclick={() => app.check({ force: true })}>⟳</button>
  <button type="button" id="theme" class="icon" title={themeTitle} aria-label="Тема" onclick={() => app.cycleTheme()}>◐</button>
  <button type="button" id="open-settings" class="icon" title="Настройки" aria-label="Настройки" onclick={() => (ui.settingsOpen = true)}>⚙</button>
</header>
