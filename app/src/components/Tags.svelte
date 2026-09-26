<!-- Теги: все теги с числом заметок; выбранный тег — его заметки. -->
<script lang="ts">
  import { notes } from "../lib/state";
  import { noteHref, tagHref } from "../lib/ids";

  let { tag }: { tag: string | null } = $props();

  const byTag = $derived.by(() => {
    const m = new Map<string, typeof notes.all>();
    for (const n of notes.all) for (const t of n.tags) m.set(t, [...(m.get(t) ?? []), n]);
    return [...m].sort(([a, x], [b, y]) => y.length - x.length || a.localeCompare(b, "ru"));
  });
  const tagged = $derived(tag ? (byTag.find(([t]) => t === tag)?.[1] ?? []) : []);

  $effect(() => {
    document.title = tag ? `#${tag} — Заметки` : "Теги — Заметки";
    document.documentElement.dataset.state = "ready";
  });
</script>

<main class="k-note" id="note">
  <div class="home tags-page">
    {#if tag}
      <p class="tags-back"><a href={tagHref()}>← все теги</a></p>
      <h1>#{tag}</h1>
      {#if tagged.length}
        <ul class="tag-notes">
          {#each tagged as n (n.id)}
            <li>
              <a href={noteHref(n.id)}>{n.title ?? n.name}</a>
              <span class="home-kind">{n.folder ? ` · ${n.folder}` : ""}{n.kind === "book" ? " · книга" : ""}</span>
              <span class="tag-others">
                {#each n.tags.filter((t) => t !== tag) as t}<a class="tag-chip" href={tagHref(t)}>#{t}</a>{/each}
              </span>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="home-lead">С таким тегом заметок нет.</p>
      {/if}
    {:else}
      <h1>Теги</h1>
      {#if byTag.length}
        <p class="tag-cloud">
          {#each byTag as [t, list] (t)}<a class="tag-chip" href={tagHref(t)}>#{t} <small>{list.length}</small></a>{/each}
        </p>
      {:else}
        <p class="home-lead">Тегов пока нет: они задаются в шаблоне заметки — <code>tags: ("сеть", "linux")</code>.</p>
      {/if}
    {/if}
  </div>
</main>
