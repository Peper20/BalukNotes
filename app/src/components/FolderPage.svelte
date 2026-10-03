<!-- Страница папки: что в ней (подпапки, заметки) и ссылка на граф её поддерева. -->
<script lang="ts">
  import { notes } from "../lib/state";
  import { folderGraphHref, folderHref, homeHref, noteHref } from "../lib/ids";
  import { plural } from "../lib/plural";
  import { countNotes, findFolder, type Folder } from "../lib/tree";

  let { path }: { path: string } = $props();

  const folder = $derived(findFolder(notes.tree, path));
  const all = (f: Folder): Folder["notes"] => [...f.notes, ...f.folders.flatMap(all)];
  const books = $derived(folder ? all(folder).filter((n) => n.kind === "book").length : 0);
  const total = $derived(folder ? countNotes(folder) : 0);

  $effect(() => {
    document.title = `${notes.folderTitle(path)} — Заметки`;
    document.documentElement.dataset.state = "ready";
  });
</script>

<main class="k-note" id="note">
  <div class="home folder-page">
    <h1>{notes.folderTitle(path)}</h1>
    {#if !folder}
      <p class="home-lead">Такой папки нет. <a href={homeHref()}>На главную</a></p>
    {:else}
      <p class="home-lead">
        {#if total}
          {total - books}
          {plural(total - books, "заметка", "заметки", "заметок")} и {books}
          {plural(books, "книга", "книги", "книг")}{folder.folders.length ? " вместе с подпапками" : ""}.
          <a class="folder-graph" href={folderGraphHref(path)}>Граф папки</a>
        {:else}
          Папка пуста.
        {/if}
      </p>
      {#if folder.folders.length}
        <h2>Папки</h2>
        <ul class="tag-notes">
          {#each folder.folders as f (f.path)}
            <li>
              <a href={folderHref(f.path)}>{f.title}</a>
              <span class="home-kind"> · {countNotes(f)} {plural(countNotes(f), "заметка", "заметки", "заметок")}</span>
            </li>
          {/each}
        </ul>
      {/if}
      {#if folder.notes.length}
        <h2>Заметки</h2>
        <ul class="tag-notes">
          {#each folder.notes as n (n.id)}
            <li>
              <a href={noteHref(n.id)}>{n.title}</a>
              {#if n.kind === "book"}<span class="home-kind"> · книга</span>{/if}
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </div>
</main>
