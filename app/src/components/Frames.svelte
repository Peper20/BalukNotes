<!--
  Кадры (`div.k-frames`): ползунок по кадрам, ‹ › и «▶». Кадры уже в
  разметке (их собрал Typst) — переключается только `data-current`,
  без перерисовки. Клавиши ←/→ — у ползунка (фокус), пробел — у кнопок.
-->
<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { nextPlaying, playStart, stepFrame, type FramesSpec } from "../lib/frames";

  let { root, items, spec }: { root: HTMLElement; items: HTMLElement[]; spec: FramesSpec } = $props();

  let current = $state(untrack(() => spec.default));
  let timer: ReturnType<typeof setInterval> | null = $state(null);
  const playing = $derived(timer !== null);
  const label = $derived(items[current]?.querySelector(".k-frames-label")?.textContent?.trim() ?? `${current + 1}`);

  $effect(() => {
    for (const [i, el] of items.entries()) el.toggleAttribute("data-current", i === current);
  });

  function stop() {
    if (timer !== null) clearInterval(timer);
    timer = null;
  }

  function toggle() {
    if (playing) return stop();
    current = playStart(current, spec);
    timer = setInterval(() => {
      const next = nextPlaying(current, spec);
      if (next === null) stop();
      else current = next;
    }, 1000 / spec.fps);
  }

  function go(delta: number) {
    stop();
    current = stepFrame(current, delta, spec);
  }

  onDestroy(() => {
    stop();
    for (const el of items) el.removeAttribute("data-current");
    delete root.dataset.live;
  });
</script>

<div class="k-frames-controls">
  <button type="button" class="k-frames-play" aria-label={playing ? "пауза" : "проиграть"} title={playing ? "Пауза" : "Проиграть"} onclick={toggle}>
    <svg viewBox="0 0 10 10" aria-hidden="true">
      {#if playing}<path d="M2 1h2v8H2zM6 1h2v8H6z" />{:else}<path d="M2 1l7 4-7 4z" />{/if}
    </svg>
  </button>
  <button type="button" class="k-frames-prev" aria-label="предыдущий кадр" onclick={() => go(-1)}>‹</button>
  <input
    type="range"
    min="0"
    max={spec.count - 1}
    step="1"
    bind:value={current}
    oninput={stop}
    aria-label={spec.name ? `параметр ${spec.name}` : "кадр"}
    aria-valuetext={label}
  />
  <button type="button" class="k-frames-next" aria-label="следующий кадр" onclick={() => go(1)}>›</button>
  <span class="k-frames-count">{current + 1} / {spec.count}</span>
</div>
