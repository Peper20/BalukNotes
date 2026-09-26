<!-- Верхняя строка: панель, путь заметки, статус и кнопки. -->
<script lang="ts">
  import FileDown from "@lucide/svelte/icons/file-down";
  import Moon from "@lucide/svelte/icons/moon";
  import PanelLeft from "@lucide/svelte/icons/panel-left";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Search from "@lucide/svelte/icons/search";
  import SettingsIcon from "@lucide/svelte/icons/settings";
  import Sun from "@lucide/svelte/icons/sun";
  import TableOfContents from "@lucide/svelte/icons/table-of-contents";
  import { nextTheme } from "../lib/appearance";
  import { app } from "../lib/app.svelte";
  import { splitId } from "../lib/ids";
  import { commands, toggleToc } from "../lib/commands.svelte";
  import { ui } from "../lib/ui.svelte";

  const run = (id: string) => commands().find((c) => c.id === id)?.run();

  const crumbs = $derived(app.currentId ? splitId(app.currentId) : null);
  const title = (name: string) => app.themes.find((t) => t.name === name)?.title ?? name;
  const next = $derived(app.themes.find((t) => t.name === nextTheme(app.theme, app.themes)));
  const themeTitle = $derived(
    `Тема: ${app.settings["appearance.theme"] === "auto" ? `как в системе (${title(app.theme)})` : title(app.theme)}. Нажать — «${next?.title ?? ""}»`,
  );
  const icon = { size: 18, strokeWidth: 1.75, "aria-hidden": true } as const;

  // PDF собирается секунды — открываем в новой вкладке, браузер покажет его сам.
  function openPdf() {
    const url = app.pdfUrl();
    if (url) open(url, "_blank");
  }
</script>

<header class="topbar">
  <button type="button" class="icon" title="Список заметок" aria-label="Список заметок" onclick={() => run("sidebar")}><PanelLeft {...icon} /></button>
  <div class="crumbs" id="crumbs">
    {#if crumbs}{crumbs.folder ? `${crumbs.folder} / ` : ""}<b>{app.currentNote?.name ?? crumbs.name}</b>{/if}
  </div>
  <span class="status" class:busy={app.busy} id="status" role="status">{app.status}</span>
  <button type="button" id="open-palette" class="icon" title="Быстрый переход и поиск (Ctrl+O)" aria-label="Поиск" onclick={() => ui.openPalette("")}><Search {...icon} /></button>
  {#if app.toc.length}
    <button type="button" id="toggle-toc" class="icon" title="Оглавление" aria-label="Оглавление" onclick={toggleToc}><TableOfContents {...icon} /></button>
  {/if}
  {#if app.page}
    <button type="button" id="pdf" class="icon" title="PDF в текущей теме" aria-label="PDF" onclick={openPdf}><FileDown {...icon} /></button>
  {/if}
  <button type="button" id="refresh" class="icon" title="Обновить (R)" aria-label="Обновить" onclick={() => app.check({ force: true })}><RefreshCw {...icon} /></button>
  <button type="button" id="theme" class="icon" title={themeTitle} aria-label="Тема" onclick={() => app.cycleTheme()}>{#if next?.dark}<Moon {...icon} />{:else}<Sun {...icon} />{/if}</button>
  <button type="button" id="open-settings" class="icon" title="Настройки" aria-label="Настройки" onclick={() => (ui.settingsOpen = true)}><SettingsIcon {...icon} /></button>
</header>
