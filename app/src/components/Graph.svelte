<!--
  Граф заметок: узлы — заметки и книги, рёбра — ссылки #see. Цвет узла — по
  папке верхнего уровня, цвета — переменные темы: граф перекрашивается вместе
  с темой. Наведение подсвечивает узел и его соседей.

  Граф живой (`graph-physics.ts`): протянутый узел тянет соседей; отпустили —
  он стоит, соседи немного отходят назад; при смене графа узлы переезжают, новые
  проявляются, ушедшие гаснут. `prefers-reduced-motion` — без движения.

  `interactive` (страница графа): колесо и два пальца — масштаб, протянуть
  фон — сдвиг. Без него (главная, граф в заметке) — картинка во всю ширину:
  узел тянется, но не дальше рамки рисунка; колесо и касание фона — странице.
  Щелчок по узлу — открыть заметку.

  Здесь — разметка и связки: жесты — `graph-gesture.ts`, переезд, появление
  и кадры — `graph-motion.ts`, физика — `graph-physics.ts`, вид — `graph-view.ts`.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { fade } from "svelte/transition";
  import type { GraphLayout } from "../lib/api";
  import { Pointers } from "../lib/graph-gesture";
  import { anyMoving, blendView, Frames, glide, introDelays } from "../lib/graph-motion";
  import { DRAG, extentOf, Physics, type Drag, type Point, type Rect } from "../lib/graph-physics";
  import { FIT_ZOOM, fitView, groupColor, LABEL_GAP, LABEL_SIZE, zoomAt, type View } from "../lib/graph-view";

  let {
    layout,
    onopen,
    interactive = false,
    highlight = null,
    titles = null,
    moved = $bindable(false),
    drag = DRAG,
  }: {
    /** Граф с раскладкой из ядра (`POST /api/graph/layout` или `#vault-graph` заметки). */
    layout: GraphLayout;
    /**
     * `background` — Ctrl+щелчок или средняя кнопка: в фоновой вкладке. Глава
     * книги — книга (`id`) на заголовке главы (`anchor`).
     */
    onopen: (id: string, background: boolean, anchor: string | null) => void;
    interactive?: boolean;
    /** Совпадения поиска: остальные узлы бледнеют. */
    highlight?: Set<string> | null;
    /** Названия заметок для подсказки. */
    titles?: Map<string, string> | null;
    /** Узлы переставлены руками — картинка разошлась с раскладкой ядра. */
    moved?: boolean;
    /** Отклик соседей на перетаскивание (настройки графа). */
    drag?: Drag;
  } = $props();

  const graph = $derived(layout);
  const center = $derived(layout.center);
  const nodeById = $derived(new Map(layout.nodes.map((n) => [n.id, n])));
  const neighbours = $derived.by(() => {
    const m = new Map<string, Set<string>>(layout.nodes.map((n) => [n.id, new Set([n.id])]));
    for (const e of layout.edges) {
      m.get(e.from)?.add(e.to);
      m.get(e.to)?.add(e.from);
    }
    return m;
  });
  const color = (id: string) => groupColor(nodeById.get(id)?.group ?? "", layout.groups);

  const base = $derived(new Map<string, Point>(layout.nodes.map((n) => [n.id, [n.x, n.y]])));
  /** Где узлы нарисованы сейчас: переезд к новой раскладке, физика. */
  let at = $state.raw(new Map<string, Point>());
  const pos = (id: string): Point => at.get(id) ?? base.get(id) ?? [0, 0];

  const reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  /** Длительность переезда и угасания, мс. */
  const GLIDE = reduced ? 0 : 500;
  const FADE = reduced ? 0 : 200;

  // Один цикл кадров на узлы графа: переезд или физика (новое отменяет старое).
  const frames = new Frames();
  $effect(() => () => frames.stop());

  // Новая раскладка (фильтр): оставшиеся узлы переезжают со своих мест.
  let sim: Physics | null = null;
  $effect(() => {
    const target = base;
    untrack(() => {
      sim = null;
      moved = false;
      const from = at;
      if (!GLIDE || !anyMoving(from, target)) {
        frames.stop();
        at = target;
        return;
      }
      frames.tween(GLIDE, (e) => (at = glide(from, target, e)));
    });
  });

  // Появление: узлы вспыхивают от середины к краям; добавленные потом — чуть позже переезда.
  const intro = untrack(() => introDelays(layout.nodes, layout.bounds));
  const delay = (id: string) => `${intro.get(id) ?? 0.15}s`;

  /** Границы нарисованного (с подписями) — в единицах раскладки. */
  const bounds = $derived(layout.bounds);

  // ── Главная: картинка во всю ширину ──────────────────────────────────
  const viewBox = $derived.by(() => {
    const [x0, y0, x1, y1] = bounds;
    const pad = 12;
    // Заметок мало (новое хранилище) — не крупнее, чем на странице графа:
    // иначе одна заметка во всю ширину. Лишнее место — по бокам.
    const w = Math.max(x1 - x0 + 2 * pad, width / FIT_ZOOM);
    return `${(x0 + x1 - w) / 2} ${y0 - pad} ${w} ${y1 - y0 + 2 * pad}`;
  });
  // Рисунок ужимается по ширине (телефон) — подписи не мельче 9 px на экране.
  let width = $state(0);
  let height = $state(0);
  const staticLabel = $derived.by(() => {
    const scale = width / Number(viewBox.split(" ")[2]);
    return scale > 0 ? Math.max(LABEL_SIZE, 9 / scale) : LABEL_SIZE;
  });


  // ── Страница графа: масштаб и сдвиг ──────────────────────────────────
  let view = $state<View>({ x: 0, y: 0, k: 1 });
  /** Вид вписан и его не двигали: поле поменяло размер (окно, панель сил) — вписать заново. */
  let snug = true;
  /** Вписать граф в окно (при смене графа и по кнопке). */
  export function fit() {
    if (width > 0 && height > 0) view = fitView(bounds, width, height);
    snug = true;
  }
  export function zoom(factor: number) {
    view = zoomAt(view, factor, width / 2, height / 2);
    snug = false;
  }
  $effect(() => {
    void [width, height];
    if (interactive && fitted !== null && untrack(() => snug)) untrack(fit);
  });
  /**
   * Вернуть раскладку ядра: узлы плавно переезжают на свои места (с
   * замедлением, без перелёта), физика забывает перестановки.
   */
  export function restore() {
    pull = null;
    sim = null;
    moved = false;
    const from = at;
    if (!GLIDE) {
      frames.stop();
      at = base;
      return;
    }
    const target = base;
    // Последний кадр — ровно раскладка (без ошибки округления).
    frames.tween(GLIDE, (e) => (at = e < 1 ? glide(from, target, e) : target));
  }
  /** Показать узел: в центр окна, не мельче 1:1. */
  export function show(id: string) {
    const [x, y] = pos(id);
    const k = Math.max(view.k, 1);
    view = { k, x: width / 2 - x * k, y: height / 2 - y * k };
    snug = false;
    focused = id;
  }
  // Новый граф (фильтр) или первый замер окна — вписать (при смене графа — плавно, вместе с узлами).
  let fitted: unknown = null;
  const viewFrames = new Frames();
  $effect(() => {
    if (!interactive || width <= 0 || height <= 0) return;
    if (fitted === base) return;
    const first = fitted === null;
    fitted = base;
    untrack(() => {
      viewFrames.stop();
      if (first || !GLIDE) return fit();
      const [from, to] = [view, fitView(bounds, width, height)];
      snug = true;
      viewFrames.tween(GLIDE, (e) => (view = blendView(from, to, e)));
    });
  });
  $effect(() => () => viewFrames.stop());

  let svg: SVGSVGElement | undefined = $state();
  let layer: SVGGElement | undefined = $state();
  const local = (e: { clientX: number; clientY: number }): Point => {
    const r = svg!.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  };
  /** Точка экрана в координатах раскладки (с масштабом страницы графа или `viewBox` картинки). */
  const world = (e: { clientX: number; clientY: number }): Point => {
    const m = layer?.getScreenCTM()?.inverse();
    if (!m) return [0, 0];
    const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(m);
    return [p.x, p.y];
  };

  // ── Физика: протянутый узел тянет соседей ────────────────────────────
  /** Рамка картинки (главная, заметка) — `viewBox`; у страницы графа рамки нет. */
  const frameRect = $derived.by((): Rect => {
    const [x0, y0, w, h] = viewBox.split(" ").map(Number) as [number, number, number, number];
    return [x0, y0, x0 + w, y0 + h];
  });
  const index = $derived(new Map(layout.nodes.map((n, i) => [n.id, i])));
  /** Корень книги — крупнее; глава — как заметка. */
  const isBook = (n: { kind: string | null; chapter: unknown }) => n.kind === "book" && !n.chapter;
  const labelSize = (n: { kind: string | null; chapter: unknown }, px: number) => (isBook(n) ? px * 1.1 : px);

  function physics(): Physics {
    sim ??= new Physics(
      layout.nodes.map((n) => {
        const [x, y] = pos(n.id);
        return { x, y, extent: extentOf(n.r, n.name, labelSize(n, LABEL_SIZE), LABEL_GAP) };
      }),
      layout.edges.flatMap((e) => {
        const [a, b] = [index.get(e.from), index.get(e.to)];
        return a == null || b == null ? [] : [[a, b] as [number, number]];
      }),
    );
    // Размеры узлов — по подписям на экране: они крупнее, чем считало ядро
    // (телефон, отдаление), и шире оценки по числу знаков.
    const drawn = new Map([...(layer?.querySelectorAll<SVGGElement>(".graph-node") ?? [])].map((g) => [g.dataset.id, g.querySelector("text")?.getBBox()]));
    const extents = layout.nodes.map((n) => {
      const e = extentOf(n.r, n.name, labelSize(n, labelPx), LABEL_GAP);
      const box = drawn.get(n.id);
      return box ? { half: Math.max(e.half, box.width / 2), top: e.top, bottom: Math.max(e.bottom, box.y + box.height) } : e;
    });
    sim.setExtents(extents);
    sim.setFrame(interactive ? null : frameRect, extents);
    sim.response = drag;
    return sim;
  }

  /** Куда тянут узел (обновляется движением указателя, применяется в кадре). */
  let pull: { i: number; to: Point } | null = null;
  function run() {
    const p = sim!;
    frames.run(() => {
      if (pull) p.drag(pull.i, ...pull.to);
      // Без движения: только сам узел, соседи стоят.
      const fastest = reduced ? 0 : p.step();
      at = new Map(layout.nodes.map((n, i) => [n.id, p.at(i)]));
      return !(reduced ? !p.holding : p.settled(fastest));
    });
  }

  const pointers = new Pointers();
  let dragging = $state(false);

  function onpointerdown(e: PointerEvent) {
    if (e.button > 1) return;
    const id = (e.target as Element).closest<SVGElement>(".graph-node")?.dataset.id;
    // Картинка: фон — странице (прокрутка, выделение), жесты — только с узла.
    if (!interactive && (!id || pointers.size > 0)) return;
    try {
      svg!.setPointerCapture(e.pointerId);
    } catch {
      return; // указателя уже нет — жеста не будет
    }
    let node = null;
    if (id) {
      const [x, y] = pos(id);
      const [wx, wy] = world(e);
      node = { id, grab: [wx - x, wy - y] as Point };
    }
    if (pointers.down(e.pointerId, local(e), view, node) === "pinch") releaseNode();
  }

  function onpointermove(e: PointerEvent) {
    const move = pointers.move(e.pointerId, local(e), (id) => index.has(id));
    if (move.kind === "view") {
      view = move.view;
      snug = false;
      if (move.pan) dragging = true;
    } else if (move.kind === "drag") {
      if (move.first || !sim) {
        dragging = true;
        moved = true;
        focused = move.id;
        physics();
      }
      const [wx, wy] = world(e);
      pull = { i: index.get(move.id)!, to: [wx - move.grab[0], wy - move.grab[1]] };
      if (!frames.active) run();
    }
  }

  /** Отпустить протянутый узел: он стоит, соседи немного отходят назад. */
  function releaseNode() {
    if (!pull) return;
    pull = null;
    sim?.release();
    if (sim && !frames.active) run();
  }

  function onpointerup(e: PointerEvent) {
    const up = pointers.up(e.pointerId, view);
    if (up.kind !== "end") return;
    const g = up.gesture;
    dragging = false;
    releaseNode();
    // Узел упёрся в рамку, а указатель ушёл дальше — подсветку снять.
    if (g?.kind === "node" && g.far && document.elementFromPoint(e.clientX, e.clientY)?.closest<SVGElement>(".graph-node")?.dataset.id !== g.id)
      focused = null;
    if (g?.kind === "node" && !g.far && e.type === "pointerup") {
      const node = graph.nodes.find((n) => n.id === g.id);
      if (node?.kind) open(g.id, e.ctrlKey || e.metaKey || e.button === 1);
    }
  }

  // Колесо — своим слушателем: у атрибута Svelte он пассивный, прокрутку страницы не отменить.
  $effect(() => {
    if (!interactive || !svg) return;
    const el = svg;
    el.addEventListener("wheel", onwheel, { passive: false });
    return () => el.removeEventListener("wheel", onwheel);
  });

  function onwheel(e: WheelEvent) {
    e.preventDefault();
    const delta = e.deltaY * (e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 400 : 1);
    const [x, y] = local(e);
    view = zoomAt(view, Math.exp(-delta * 0.0015), x, y);
    snug = false;
  }

  /** Открыть узел: заметку, книгу или книгу на главе. */
  function open(id: string, background: boolean) {
    const n = nodeById.get(id);
    if (n?.chapter) onopen(n.chapter.book, background, n.chapter.anchor);
    else onopen(id, background, null);
  }

  function onkeydown(e: KeyboardEvent, id: string) {
    if (e.key === "Enter") open(id, e.ctrlKey || e.metaKey);
  }

  let focused = $state<string | null>(null);
  const near = $derived(focused ? neighbours.get(focused) : null);
  /** Подписи на экране не мельче 8 px; совсем мелко — прячутся (кроме книг, найденных и соседей наведённого). */
  const labelPx = $derived(interactive ? Math.max(LABEL_SIZE, 8 / view.k) : staticLabel);
  const far = $derived(interactive && view.k < 0.5);
</script>

{#if graph.nodes.length}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <svg
    bind:this={svg}
    class="graph-svg"
    class:interactive
    class:dragging
    class:focused
    class:searching={highlight != null}
    class:far
    viewBox={interactive ? null : viewBox}
    role="img"
    aria-label="Граф заметок"
    bind:clientWidth={width}
    bind:clientHeight={height}
    style:--label="{labelPx}px"
    {onpointerdown}
    {onpointermove}
    {onpointerup}
    onpointercancel={onpointerup}
  >
    <g bind:this={layer} transform={interactive ? `translate(${view.x} ${view.y}) scale(${view.k})` : null}>
      {#each graph.edges as e (`${e.from}\n${e.to}`)}
        {@const a = pos(e.from)}
        {@const b = pos(e.to)}
        <line
          out:fade={{ duration: FADE }}
          style:--delay={delay(intro.has(e.from) && intro.has(e.to) ? (intro.get(e.from)! > intro.get(e.to)! ? e.from : e.to) : "")}
          x1={a[0]}
          y1={a[1]}
          x2={b[0]}
          y2={b[1]}
          class="graph-edge"
          class:chapter={e.chapter}
          class:near={focused != null && (e.from === focused || e.to === focused)}
          stroke-width={Math.min(1 + e.count * 0.4, 3)}
        />
      {/each}
      {#each graph.nodes as n (n.id)}
        {@const [x, y] = pos(n.id)}
        {@const r = n.r}
        <!-- роль задана условно (на странице графа — ссылка), проверка этого не видит -->
        <!-- svelte-ignore a11y_no_static_element_interactions, a11y_no_noninteractive_tabindex -->
        <g
          out:fade={{ duration: FADE }}
          style:--delay={delay(n.id)}
          class="graph-node"
          class:missing={!n.kind}
          class:book={isBook(n)}
          class:chapter={n.chapter != null}
          class:near={near?.has(n.id)}
          class:hit={highlight?.has(n.id)}
          class:center={n.id === center}
          transform="translate({x} {y})"
          data-id={n.id}
          role={interactive && n.kind ? "link" : undefined}
          tabindex={interactive && n.kind ? 0 : undefined}
          onkeydown={(e) => n.kind && onkeydown(e, n.id)}
          onpointerenter={() => !dragging && (focused = n.id)}
          onpointerleave={() => !dragging && (focused = null)}
        >
          <!-- Глава книги — полый кружок цвета книги: часть книги, а не заметка. -->
          <circle {r} style:fill={n.chapter ? "var(--k-surface)" : n.kind ? color(n.id) : "none"} style:stroke={color(n.id)} />
          <text y={r + LABEL_GAP + labelPx * 0.9} text-anchor="middle">{n.name}</text>
          <title>
            {n.chapter
              ? `Глава «${n.name}» · ${titles?.get(n.chapter.book) ?? n.chapter.book} · связей: ${n.degree}`
              : n.kind
                ? `${titles?.get(n.id) ?? n.id}${n.kind === "book" ? " (книга)" : ""} · связей: ${n.degree}`
                : `${n.id} — такой заметки нет`}
          </title>
        </g>
      {/each}
    </g>
  </svg>
{/if}
