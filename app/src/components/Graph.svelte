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
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { fade } from "svelte/transition";
  import type { GraphLayout } from "../lib/api";
  import { extentOf, Physics, type Point, type Rect } from "../lib/graph-physics";
  import { fitView, groupColor, LABEL_GAP, LABEL_SIZE, zoomAt, type View } from "../lib/graph-view";

  let {
    layout,
    onopen,
    interactive = false,
    highlight = null,
    titles = null,
  }: {
    /** Граф с раскладкой из ядра (`POST /api/graph/layout` или `#vault-graph` заметки). */
    layout: GraphLayout;
    onopen: (id: string, newTab: boolean) => void;
    interactive?: boolean;
    /** Совпадения поиска: остальные узлы бледнеют. */
    highlight?: Set<string> | null;
    /** Названия заметок для подсказки. */
    titles?: Map<string, string> | null;
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
  const ease = (t: number) => 1 - (1 - t) ** 3;

  // Один кадр анимации на граф: переезд или физика (новое отменяет старое).
  let frame = 0;
  function animate(tick: (now: number) => boolean) {
    cancelAnimationFrame(frame);
    const run = (now: number) => (frame = tick(now) ? requestAnimationFrame(run) : 0);
    frame = requestAnimationFrame(run);
  }
  $effect(() => () => cancelAnimationFrame(frame));

  // Новая раскладка (фильтр): оставшиеся узлы переезжают со своих мест.
  let sim: Physics | null = null;
  $effect(() => {
    const target = base;
    untrack(() => {
      sim = null;
      const from = at;
      const moving = [...target].filter(([id]) => from.has(id));
      if (!GLIDE || !moving.length) {
        cancelAnimationFrame(frame);
        at = target;
        return;
      }
      const start = performance.now();
      animate((now) => {
        const t = Math.min((now - start) / GLIDE, 1);
        const e = ease(t);
        const next = new Map(target);
        for (const [id, [x, y]] of moving) {
          const [fx, fy] = from.get(id)!;
          next.set(id, [fx + (x - fx) * e, fy + (y - fy) * e]);
        }
        at = next;
        return t < 1;
      });
    });
  });

  // Появление: узлы вспыхивают от середины к краям; добавленные потом — чуть позже переезда.
  const intro = untrack(() => {
    const [x0, y0, x1, y1] = layout.bounds;
    const [cx, cy, far] = [(x0 + x1) / 2, (y0 + y1) / 2, Math.max(Math.hypot(x1 - x0, y1 - y0) / 2, 1)];
    return new Map(layout.nodes.map((n) => [n.id, Math.min(Math.hypot(n.x - cx, n.y - cy) / far, 1) * 0.35]));
  });
  const delay = (id: string) => `${intro.get(id) ?? 0.15}s`;

  /** Границы нарисованного (с подписями) — в единицах раскладки. */
  const bounds = $derived(layout.bounds);

  // ── Главная: картинка во всю ширину ──────────────────────────────────
  const viewBox = $derived.by(() => {
    const [x0, y0, x1, y1] = bounds;
    const pad = 12;
    return `${x0 - pad} ${y0 - pad} ${x1 - x0 + 2 * pad} ${y1 - y0 + 2 * pad}`;
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
  /** Вписать граф в окно (при смене графа и по кнопке). */
  export function fit() {
    if (width > 0 && height > 0) view = fitView(bounds, width, height);
  }
  export function zoom(factor: number) {
    view = zoomAt(view, factor, width / 2, height / 2);
  }
  /** Показать узел: в центр окна, не мельче 1:1. */
  export function show(id: string) {
    const [x, y] = pos(id);
    const k = Math.max(view.k, 1);
    view = { k, x: width / 2 - x * k, y: height / 2 - y * k };
    focused = id;
  }
  // Новый граф (фильтр) или первый замер окна — вписать (при смене графа — плавно, вместе с узлами).
  let fitted: unknown = null;
  let viewFrame = 0;
  $effect(() => {
    if (!interactive || width <= 0 || height <= 0) return;
    if (fitted === base) return;
    const first = fitted === null;
    fitted = base;
    untrack(() => {
      cancelAnimationFrame(viewFrame);
      if (first || !GLIDE) return fit();
      const [from, to, start] = [view, fitView(bounds, width, height), performance.now()];
      const step = (now: number) => {
        const t = Math.min((now - start) / GLIDE, 1);
        const e = ease(t);
        view = { x: from.x + (to.x - from.x) * e, y: from.y + (to.y - from.y) * e, k: from.k + (to.k - from.k) * e };
        viewFrame = t < 1 ? requestAnimationFrame(step) : 0;
      };
      viewFrame = requestAnimationFrame(step);
    });
  });
  $effect(() => () => cancelAnimationFrame(viewFrame));

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
  const labelSize = (n: { kind: string | null }, px: number) => (n.kind === "book" ? px * 1.1 : px);

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
    return sim;
  }

  /** Куда тянут узел (обновляется движением указателя, применяется в кадре). */
  let pull: { i: number; to: Point } | null = null;
  function run() {
    const p = sim!;
    animate(() => {
      if (pull) p.drag(pull.i, ...pull.to);
      // Без движения: только сам узел, соседи стоят.
      const fastest = reduced ? 0 : p.step();
      at = new Map(layout.nodes.map((n, i) => [n.id, p.at(i)]));
      return !(reduced ? !p.holding : p.settled(fastest));
    });
  }

  type Gesture =
    | { kind: "pan"; start: Point; view: View }
    | { kind: "node"; id: string; start: Point; grab: Point; far: boolean }
    | { kind: "pinch"; dist: number; mid: Point; view: View };
  const pointers = new Map<number, Point>();
  let gesture: Gesture | null = null;
  let dragging = $state(false);

  function pinchStart(): Gesture {
    const [a, b] = [...pointers.values()] as [Point, Point];
    return { kind: "pinch", dist: Math.max(Math.hypot(a[0] - b[0], a[1] - b[1]), 1), mid: [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2], view };
  }

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
    const p = local(e);
    pointers.set(e.pointerId, p);
    if (pointers.size === 2) {
      releaseNode();
      gesture = pinchStart();
      return;
    }
    if (pointers.size > 2) return;
    if (id) {
      const [x, y] = pos(id);
      const [wx, wy] = world(e);
      gesture = { kind: "node", id, start: p, grab: [wx - x, wy - y], far: false };
    } else gesture = { kind: "pan", start: p, view };
  }

  function onpointermove(e: PointerEvent) {
    if (!pointers.has(e.pointerId) || !gesture) return;
    const p = local(e);
    pointers.set(e.pointerId, p);
    if (gesture.kind === "pinch" && pointers.size === 2) {
      const [a, b] = [...pointers.values()] as [Point, Point];
      const mid: Point = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
      const v = zoomAt(gesture.view, Math.hypot(a[0] - b[0], a[1] - b[1]) / gesture.dist, gesture.mid[0], gesture.mid[1]);
      view = { ...v, x: v.x + mid[0] - gesture.mid[0], y: v.y + mid[1] - gesture.mid[1] };
    } else if (gesture.kind === "pan") {
      view = { ...gesture.view, x: gesture.view.x + p[0] - gesture.start[0], y: gesture.view.y + p[1] - gesture.start[1] };
      dragging = true;
    } else if (gesture.kind === "node") {
      if (!gesture.far && Math.hypot(p[0] - gesture.start[0], p[1] - gesture.start[1]) < 4) return;
      const i = index.get(gesture.id);
      if (i == null) return;
      if (!gesture.far || !sim) {
        gesture.far = true;
        dragging = true;
        focused = gesture.id;
        physics();
      }
      const [wx, wy] = world(e);
      pull = { i, to: [wx - gesture.grab[0], wy - gesture.grab[1]] };
      if (!frame) run();
    }
  }

  /** Отпустить протянутый узел: он стоит, соседи немного отходят назад. */
  function releaseNode() {
    if (!pull) return;
    pull = null;
    sim?.release();
    if (sim && !frame) run();
  }

  function onpointerup(e: PointerEvent) {
    if (!pointers.delete(e.pointerId)) return;
    const g = gesture;
    if (pointers.size === 1 && g?.kind === "pinch") {
      // остался один палец — дальше сдвиг от него
      gesture = { kind: "pan", start: [...pointers.values()][0]!, view };
      return;
    }
    if (pointers.size > 0) return;
    gesture = null;
    dragging = false;
    releaseNode();
    // Узел упёрся в рамку, а указатель ушёл дальше — подсветку снять.
    if (g?.kind === "node" && g.far && document.elementFromPoint(e.clientX, e.clientY)?.closest<SVGElement>(".graph-node")?.dataset.id !== g.id)
      focused = null;
    if (g?.kind === "node" && !g.far && e.type === "pointerup") {
      const node = graph.nodes.find((n) => n.id === g.id);
      if (node?.kind) onopen(g.id, e.ctrlKey || e.metaKey || e.button === 1);
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
  }

  function onkeydown(e: KeyboardEvent, id: string) {
    if (e.key === "Enter") onopen(id, e.ctrlKey || e.metaKey);
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
          class:book={n.kind === "book"}
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
          <circle {r} style:fill={n.kind ? color(n.id) : "none"} style:stroke={color(n.id)} />
          <text y={r + LABEL_GAP + labelPx * 0.9} text-anchor="middle">{n.name}</text>
          <title>
            {n.kind
              ? `${titles?.get(n.id) ?? n.id}${n.kind === "book" ? " (книга)" : ""} · связей: ${n.degree}`
              : `${n.id} — такой заметки нет`}
          </title>
        </g>
      {/each}
    </g>
  </svg>
{/if}
