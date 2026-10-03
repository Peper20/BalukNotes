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
  import CloudOff from "@lucide/svelte/icons/cloud-off";
  import { connection, notes, reader, router, settings, updates } from "../lib/state";
  import { folderHref } from "../lib/ids";
  import { ancestors } from "../lib/tree";
  import { commands, toggleToc } from "../lib/commands.svelte";
  import { ui } from "../lib/ui.svelte";

  const run = (id: string) => commands().find((c) => c.id === id)?.run();

  /** Путь в верхней строке: папки (ссылки) и сама заметка или папка. */
  const crumb = $derived.by(() => {
    const r = router.route;
    if (r.kind === "note") return { folders: ancestors(r.id), title: notes.title(r.id) };
    if (r.kind === "folder") return { folders: ancestors(r.path), title: notes.folderTitle(r.path) };
    return null;
  });
  const title = (name: string) => settings.themes.find((t) => t.name === name)?.title ?? name;
  const next = $derived(settings.themes.find((t) => t.name === nextTheme(settings.theme, settings.themes)));
  const themeTitle = $derived(
    `Тема: ${settings.values["appearance.theme"] === "auto" ? `как в системе (${title(settings.theme)})` : title(settings.theme)}. Нажать — «${next?.title ?? ""}»`,
  );
  const icon = { size: 18, strokeWidth: 1.75, "aria-hidden": true } as const;

  // PDF собирается секунды — открываем в новой вкладке, браузер покажет его сам.
  function openPdf() {
    const url = reader.pdfUrl();
    if (url) open(url, "_blank");
  }
</script>

<header class="topbar">
  <button type="button" class="icon" title="Список заметок" aria-label="Список заметок" onclick={() => run("sidebar")}><PanelLeft {...icon} /></button>
  <div class="crumbs" id="crumbs">
    <!-- Папки пути - ссылки на их страницы. -->
    {#if crumb}{#each crumb.folders as path (path)}<a class="crumb" href={folderHref(path)}
          >{notes.folderTitle(path)}</a
        >{" / "}{/each}<b>{crumb.title}</b>{/if}
  </div>
  <span class="status" class:busy={reader.busy} id="status" role="status">{reader.status}</span>
  {#if !connection.online}
    <!-- Нет связи с сервером (state/connection): метка, подробности — в подсказке. -->
    <button
      type="button"
      class="offline-chip"
      id="offline"
      title="Нет связи с сервером. Показано то, что уже загружено; когда связь появится, страница обновится. Нажмите, чтобы проверить сейчас."
      onclick={() => connection.retry()}
    >
      <CloudOff size={15} strokeWidth={1.75} aria-hidden="true" />нет связи
    </button>
  {/if}
  <button type="button" id="open-palette" class="icon" title="Быстрый переход и поиск (Ctrl+O)" aria-label="Поиск" onclick={() => ui.openPalette("")}><Search {...icon} /></button>
  {#if reader.toc.length}
    <button type="button" id="toggle-toc" class="icon" title="Оглавление" aria-label="Оглавление" onclick={toggleToc}><TableOfContents {...icon} /></button>
  {/if}
  {#if reader.page}
    <button type="button" id="pdf" class="icon" title="PDF в текущей теме" aria-label="PDF" onclick={openPdf}><FileDown {...icon} /></button>
  {/if}
  <button type="button" id="refresh" class="icon" title="Обновить (R)" aria-label="Обновить" onclick={() => updates.check({ force: true })}><RefreshCw {...icon} /></button>
  <button type="button" id="theme" class="icon" title={themeTitle} aria-label="Тема" onclick={() => settings.cycleTheme()}>{#if next?.dark}<Moon {...icon} />{:else}<Sun {...icon} />{/if}</button>
  <button type="button" id="open-settings" class="icon" title="Настройки" aria-label="Настройки" onclick={() => (ui.settingsOpen = true)}><SettingsIcon {...icon} /></button>
</header>
