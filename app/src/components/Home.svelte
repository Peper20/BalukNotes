<!-- Главная: сколько заметок, граф связей и список по папкам. -->
<script lang="ts">
  import { api, type GraphLayout } from "../lib/api";
  import { app } from "../lib/app.svelte";
  import { noteHref } from "../lib/ids";
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

  const books = $derived(app.notes.filter((n) => n.kind === "book").length);
  const byFolder = $derived(Map.groupBy(app.notes, (n) => n.folder || "—"));
  const plural = (n: number, one: string, few: string, many: string) => {
    const m10 = n % 10;
    const m100 = n % 100;
    return m10 === 1 && m100 !== 11 ? one : m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14) ? few : many;
  };
</script>

<main class="k-note" id="note">
  <div class="home">
    <h1>Заметки</h1>
    {#if !app.notes.length}
      <p>Хранилище пусто: положите .typ-файлы в data/vault/.</p>
    {:else}
      <p class="home-lead">
        {app.notes.length - books}
        {plural(app.notes.length - books, "заметка", "заметки", "заметок")} и {books}
        {plural(books, "книга", "книги", "книг")}.
        {#if app.notes.some((n) => n.id === "Начало")}
          Начните с <a href={noteHref("Начало")}>экскурсии по возможностям</a>.
        {/if}
      </p>
      {#if !graphFailed}
        <section class="graph">
          {#if graph}
            <Graph layout={graph} onopen={(id, newTab) => app.open(id, null, { newTab })} />
            <GraphLegend groups={graph.groups} />
          {/if}
        </section>
        <p class="graph-hint">
          Наведите на узел — подсветятся его связи; протяните — соседи потянутся за ним; нажмите — откроется заметка.
          Крупные узлы — книги, пустой — заметка,
          на которую ссылаются, но её ещё нет. <a href="/graph">Граф целиком</a> — масштаб, фильтры, соседи заметки.
        </p>
      {/if}
      <h2>Все заметки</h2>
      <div class="home-list">
        {#each byFolder as [folder, notes] (folder)}
          <div>
            <h3>{folder}</h3>
            <ul>
              {#each notes as n (n.id)}
                <li>
                  <a href={noteHref(n.id)}>{n.name}</a>
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
