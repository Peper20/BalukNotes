<!-- Главная: сколько заметок, граф связей и список по папкам. -->
<script lang="ts">
  import { api, type GraphLayout } from "../lib/api";
  import { notes, router } from "../lib/state";
  import { graphHref, noteHref } from "../lib/ids";
  import { plural } from "../lib/plural";
  import { vault } from "../lib/vault";
  import Graph from "./Graph.svelte";
  import GraphLegend from "./GraphLegend.svelte";

  let graph = $state.raw<GraphLayout | null>(null);
  let graphFailed = $state(false);

  $effect(() => {
    document.documentElement.dataset.state = "loading";
    api
      .graphLayout()
      .then((g) => (graph = g))
      .catch(() => (graphFailed = true))
      .finally(() => requestAnimationFrame(() => (document.documentElement.dataset.state = "ready")));
  });

  const books = $derived(notes.all.filter((n) => n.kind === "book").length);
  const byTitle = (a: string, b: string) => a.localeCompare(b, "ru", { numeric: true });
  // Папка — названиями (`Учёба / Матан`), заметки в ней — по названию.
  const byFolder = $derived(
    [...Map.groupBy(notes.all, (n) => (n.folder ? notes.folderLabel(n.folder) : "—"))]
      .map(([folder, list]) => [folder, list.toSorted((a, b) => byTitle(a.title, b.title))] as const)
      .sort(([a], [b]) => byTitle(a, b)),
  );
</script>

<main class="k-note" id="note">
  <div class="home">
    <h1>{vault()}</h1>
    {#if !notes.all.length}
      <p>Хранилище пусто: новая заметка — <code>notes new --vault "{vault()}" --title "Название"</code> (где хранилище — <code>notes info</code>).</p>
    {:else}
      <p class="home-lead">
        {notes.all.length - books}
        {plural(notes.all.length - books, "заметка", "заметки", "заметок")} и {books}
        {plural(books, "книга", "книги", "книг")}.
        {#if notes.all.some((n) => n.id === "Начало")}
          Начните с <a href={noteHref("Начало")}>экскурсии по возможностям</a>.
        {/if}
      </p>
      {#if !graphFailed}
        <section class="graph">
          {#if graph}
            <Graph layout={graph} onopen={(id, background, anchor) => router.open(id, anchor, { background })} />
            <GraphLegend groups={graph.groups} />
          {/if}
        </section>
        <p class="graph-hint">
          Наведите на узел — подсветятся его связи; протяните — соседи потянутся за ним; нажмите — откроется заметка.
          Крупные узлы — книги, пустой — заметка,
          на которую ссылаются, но её ещё нет. <a href={graphHref()}>Граф целиком</a> — масштаб, фильтры, соседи заметки.
        </p>
      {/if}
      <h2>Все заметки</h2>
      <div class="home-list">
        {#each byFolder as [folder, list] (folder)}
          <div>
            <h3>{folder}</h3>
            <ul>
              {#each list as n (n.id)}
                <li>
                  <a href={noteHref(n.id)}>{n.title}</a>
                  {#if n.kind === "book"}<span class="home-kind"> книга</span>{/if}
                </li>
              {/each}
            </ul>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</main>
