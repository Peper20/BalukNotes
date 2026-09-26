<!--
  Палитра: быстрый переход (Ctrl+O), команды (Ctrl+K или «>»), поиск по
  тексту (Ctrl+Shift+F или «/»), теги («#»). ↑↓ — выбор, Enter — открыть,
  Ctrl+Enter — в новой вкладке, Esc — закрыть.
-->
<script lang="ts">
  import { tick } from "svelte";
  import { api, type NoteListItem, type SearchHit } from "../lib/api";
  import { notes, places, router } from "../lib/state";
  import { commands, openNote, type Command } from "../lib/commands.svelte";
  import { fuzzy, highlight } from "../lib/fuzzy";
  import { tagHref } from "../lib/ids";
  import { ui } from "../lib/ui.svelte";

  type Item =
    | { kind: "note"; note: NoteListItem; title: number[]; path: number[] }
    | { kind: "hit"; hit: SearchHit }
    | { kind: "command"; command: Command; positions: number[] }
    | { kind: "tag"; tag: string; count: number; positions: number[] };

  let input: HTMLInputElement | undefined = $state();
  let list: HTMLUListElement | undefined = $state();
  let selected = $state(0);
  let hits = $state.raw<SearchHit[]>([]);
  let searching = $state(false);

  const query = $derived(ui.palette?.query ?? "");
  const mode = $derived(query.startsWith(">") ? "commands" : query.startsWith("/") ? "text" : query.startsWith("#") ? "tags" : "notes");
  const q = $derived(mode === "notes" ? query.trim() : query.slice(1).trim());

  const titleOf = (n: NoteListItem) => n.title ?? n.name;

  const noteItems = $derived.by((): Item[] => {
    if (mode !== "notes") return [];
    if (!q) {
      const recent = places.recent.map((id) => notes.all.find((n) => n.id === id)).filter((n) => n != null);
      const rest = notes.all.filter((n) => !places.recent.includes(n.id));
      return [...recent, ...rest].slice(0, 30).map((note) => ({ kind: "note", note, title: [], path: [] }));
    }
    return notes.all
      .map((note) => {
        const t = fuzzy(q, titleOf(note));
        const p = fuzzy(q, note.id);
        const score = Math.max(t ? t.score + 5 : -Infinity, p ? p.score : -Infinity);
        return { note, t, p, score };
      })
      .filter((x) => x.score > -Infinity)
      .sort((a, b) => b.score - a.score)
      .slice(0, 20)
      .map(({ note, t, p }) => ({ kind: "note", note, title: t?.positions ?? [], path: t ? [] : (p?.positions ?? []) }));
  });

  const commandItems = $derived.by((): Item[] => {
    if (mode !== "commands") return [];
    return commands()
      .filter((c) => c.available?.() ?? true)
      .map((command) => ({ command, m: fuzzy(q, command.title) }))
      .filter((x) => x.m)
      .sort((a, b) => (q ? b.m!.score - a.m!.score : 0))
      .map(({ command, m }) => ({ kind: "command", command, positions: m!.positions }));
  });

  const tagItems = $derived.by((): Item[] => {
    if (mode !== "tags") return [];
    const counts = new Map<string, number>();
    for (const n of notes.all) for (const t of n.tags) counts.set(t, (counts.get(t) ?? 0) + 1);
    return [...counts]
      .map(([tag, count]) => ({ tag, count, m: fuzzy(q, tag) }))
      .filter((x) => x.m)
      .sort((a, b) => b.m!.score - a.m!.score || b.count - a.count)
      .map(({ tag, count, m }) => ({ kind: "tag", tag, count, positions: m!.positions }));
  });

  const hitItems = $derived<Item[]>(mode === "notes" || mode === "text" ? hits.map((hit) => ({ kind: "hit", hit })) : []);
  const items = $derived([...noteItems, ...commandItems, ...tagItems, ...hitItems]);

  // Поиск по тексту — с задержкой и отменой прежнего запроса.
  $effect(() => {
    const text = mode === "notes" || mode === "text" ? q : "";
    hits = [];
    if (text.length < 2) return;
    const ctrl = new AbortController();
    const timer = setTimeout(() => {
      searching = true;
      api
        .search(text, ctrl.signal, mode === "text" ? 50 : 15)
        .then((h) => (hits = h))
        .catch(() => {})
        .finally(() => (searching = false));
    }, 120);
    return () => {
      clearTimeout(timer);
      ctrl.abort();
    };
  });

  $effect(() => {
    void query;
    selected = 0;
  });

  $effect(() => {
    if (ui.palette && input) {
      input.focus();
      // Префикс режима — выделить нельзя, курсор — в конец.
      input.setSelectionRange(input.value.length, input.value.length);
    }
  });

  function close() {
    ui.palette = null;
  }

  function choose(item: Item | undefined, newTab = false) {
    if (!item) return;
    close();
    if (item.kind === "note") openNote(item.note.id, null, newTab);
    else if (item.kind === "hit") openNote(item.hit.id, item.hit.anchor, newTab);
    else if (item.kind === "tag") router.go(tagHref(item.tag), { newTab });
    else item.command.run();
  }

  async function move(delta: number) {
    if (!items.length) return;
    selected = (selected + delta + items.length) % items.length;
    await tick();
    list?.querySelector(".selected")?.scrollIntoView({ block: "nearest" });
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") move(1);
    else if (e.key === "ArrowUp") move(-1);
    else if (e.key === "PageDown") move(8);
    else if (e.key === "PageUp") move(-8);
    else if (e.key === "Enter") choose(items[selected], e.ctrlKey || e.metaKey);
    else if (e.key === "Escape") close();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  const groupOf = (item: Item) =>
    item.kind === "note" ? (q ? "Заметки" : "Недавние и все заметки") : item.kind === "hit" ? "В тексте" : item.kind === "tag" ? "Теги" : "Команды";
  const placeholder = $derived(
    mode === "commands" ? "Команда…" : mode === "text" ? "Слова из текста заметок…" : mode === "tags" ? "Тег…" : "Заметка или слова из текста…",
  );
</script>

{#snippet marked(text: string, positions: number[])}
  {#each highlight(text, positions) as part}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
{/snippet}

{#if ui.palette}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="palette-backdrop" onclick={close}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="palette" role="dialog" aria-label="Быстрый переход" tabindex="-1" onclick={(e) => e.stopPropagation()}>
      <input
        bind:this={input}
        bind:value={ui.palette.query}
        onkeydown={onKeydown}
        {placeholder}
        spellcheck="false"
        autocomplete="off"
        aria-label="Запрос"
        aria-controls="palette-list"
      />
      <ul class="palette-list" id="palette-list" role="listbox" bind:this={list}>
        {#each items as item, i}
          {#if i === 0 || groupOf(items[i - 1]!) !== groupOf(item)}
            <li class="palette-group" role="presentation">
              {groupOf(item)}{item.kind === "hit" ? ` · ${hits.length}` : ""}
            </li>
          {/if}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <li
            role="option"
            aria-selected={i === selected}
            class:selected={i === selected}
            onmousemove={() => (selected = i)}
            onclick={(e) => choose(item, e.ctrlKey || e.metaKey)}
          >
            {#if item.kind === "note"}
              <span class="p-title">{@render marked(titleOf(item.note), item.title)}</span>
              {#if item.note.kind === "book"}<span class="p-badge">книга</span>{/if}
              <span class="p-path">{@render marked(item.note.id, item.path)}</span>
            {:else if item.kind === "hit"}
              <span class="p-title">{item.hit.title}{#if item.hit.heading}<span class="p-heading">{` › ${item.hit.heading}`}</span>{/if}</span>
              <span class="p-snippet">{#each item.hit.snippet as f}{#if f.hit}<mark>{f.text}</mark>{:else}{f.text}{/if}{/each}</span>
            {:else if item.kind === "tag"}
              <span class="p-title">#{@render marked(item.tag, item.positions)}</span>
              <span class="p-path">{item.count}</span>
            {:else}
              <span class="p-title">{@render marked(item.command.title, item.positions)}</span>
              {#if item.command.keys?.[0]}<kbd>{item.command.keys[0].label}</kbd>{/if}
            {/if}
          </li>
        {:else}
          <li class="palette-empty" role="presentation">
            {searching ? "Ищу…" : q ? "Ничего не нашлось" : mode === "text" ? "Введите хотя бы две буквы" : "Пусто"}
          </li>
        {/each}
      </ul>
      <footer class="palette-help">
        <span><kbd>↑</kbd><kbd>↓</kbd> выбор</span>
        <span><kbd>Enter</kbd> открыть</span>
        <span><kbd>Ctrl</kbd>+<kbd>Enter</kbd> во вкладке</span>
        <span><kbd>&gt;</kbd> команды</span>
        <span><kbd>/</kbd> текст</span>
        <span><kbd>#</kbd> теги</span>
      </footer>
    </div>
  </div>
{/if}
