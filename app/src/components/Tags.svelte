<!-- Теги: все теги с числом заметок; выбранный тег — его заметки и главы книг. -->
<script lang="ts">
  import { notes } from "../lib/state";
  import { noteHref, tagHref } from "../lib/ids";
  import { noteTags, tagIndex } from "../lib/tags";

  let { tag }: { tag: string | null } = $props();

  const byTag = $derived(tagIndex(notes.all));
  const tagged = $derived(tag ? (byTag.find((e) => e.tag === tag)?.places ?? []) : []);

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
          {#each tagged as { note: n, chapter: ch } (`${n.id}#${ch?.anchor ?? ""}`)}
            <li>
              {#if ch}
                <a href={noteHref(n.id, ch.anchor)}>{ch.title}</a>
                <span class="home-kind"> · глава книги «{n.title}»</span>
              {:else}
                <a href={noteHref(n.id)}>{n.title}</a>
                <span class="home-kind">{n.folder ? ` · ${notes.folderLabel(n.folder)}` : ""}{n.kind === "book" ? " · книга" : ""}</span>
              {/if}
              <span class="tag-others">
                {#each (ch ? ch.tags : noteTags(n)).filter((t) => t !== tag) as t}<a class="tag-chip" href={tagHref(t)}>#{t}</a>{/each}
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
          {#each byTag as e (e.tag)}<a class="tag-chip" href={tagHref(e.tag)}>#{e.tag} <small>{e.notes}</small></a>{/each}
        </p>
      {:else}
        <p class="home-lead">Тегов пока нет: они задаются в шаблоне заметки — <code>tags: ("сеть", "linux")</code>.</p>
      {/if}
    {/if}
  </div>
</main>
