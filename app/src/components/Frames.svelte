<!--
  Frames (`div.k-frames`): a slider over the frames, "назад"/"вперёд" and
  "проиграть". The frames are already in the markup (Typst built them), only
  `data-current` switches, without redrawing. The ←/→ keys belong to the
  slider (focus), space to the buttons.
-->
<script lang="ts">
  import ChevronLeft from "@lucide/svelte/icons/chevron-left";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Pause from "@lucide/svelte/icons/pause";
  import Play from "@lucide/svelte/icons/play";
  import { onDestroy, untrack } from "svelte";
  import { nextPlaying, playStart, stepFrame, type FramesSpec } from "../lib/frames";

  let { root, items, spec }: { root: HTMLElement; items: HTMLElement[]; spec: FramesSpec } = $props();

  let current = $state(untrack(() => spec.default));
  let timer: ReturnType<typeof setInterval> | null = $state(null);
  const playing = $derived(timer !== null);
  const icon = { size: "1.2em", strokeWidth: 1.75, "aria-hidden": true } as const;
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
    {#if playing}<Pause {...icon} />{:else}<Play {...icon} />{/if}
  </button>
  <button type="button" class="k-frames-prev" aria-label="предыдущий кадр" title="Предыдущий кадр" onclick={() => go(-1)}><ChevronLeft {...icon} /></button>
  <input
    type="range"
    min="0"
    max={spec.count - 1}
    step="1"
    bind:value={current}
    style:--k-fill="{(current / Math.max(spec.count - 1, 1)) * 100}%"
    oninput={stop}
    aria-label={spec.name ? `параметр ${spec.name}` : "кадр"}
    aria-valuetext={label}
  />
  <button type="button" class="k-frames-next" aria-label="следующий кадр" title="Следующий кадр" onclick={() => go(1)}><ChevronRight {...icon} /></button>
  <span class="k-frames-count">{current + 1} / {spec.count}</span>
</div>
