<!--
  The note graph: nodes are notes and books, edges are #see links. A node's
  color is by its top-level folder, the colors are theme variables: the graph
  recolors with the theme. Hover highlights a node and its neighbours.

  The graph is live (`graph-physics.ts`): a dragged node pulls its
  neighbours; released, it stays and the neighbours go a bit back; when the
  graph changes, nodes move, new ones fade in, gone ones fade out.
  `prefers-reduced-motion` - no motion.

  `interactive` (the graph page): the wheel and two fingers zoom, dragging the
  background pans. Without it (home, a graph in a note) - a full-width
  picture: a node can be dragged, but not past the figure frame; the wheel
  and touching the background go to the page. A click on a node opens the
  note.

  Here are the markup and the glue: gestures - `graph-gesture.ts`, moving,
  appearing and frames - `graph-motion.ts`, physics - `graph-physics.ts`,
  the view - `graph-view.ts`.
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
    /** The graph with the layout from the core (`POST /api/graph/layout` or a note's `#vault-graph`). */
    layout: GraphLayout;
    /**
     * `background` - Ctrl+click or the middle button: in a background tab. A
     * book chapter is the book (`id`) at the chapter heading (`anchor`).
     */
    onopen: (id: string, background: boolean, anchor: string | null) => void;
    interactive?: boolean;
    /** Search matches: the other nodes fade. */
    highlight?: Set<string> | null;
    /** Note titles for the tooltip. */
    titles?: Map<string, string> | null;
    /** Nodes were moved by hand: the picture differs from the core layout. */
    moved?: boolean;
    /** Neighbours' response to dragging (graph settings). */
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
  /** Where the nodes are drawn now: moving to a new layout, physics. */
  let at = $state.raw(new Map<string, Point>());
  const pos = (id: string): Point => at.get(id) ?? base.get(id) ?? [0, 0];

  const reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  /** Duration of moving and fading, ms. */
  const GLIDE = reduced ? 0 : 500;
  const FADE = reduced ? 0 : 200;

  // One frame loop for the graph nodes: moving or physics (the new one cancels the old).
  const frames = new Frames();
  $effect(() => () => frames.stop());

  // A new layout (filter): the remaining nodes move from their places.
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

  // Appearing: nodes light up from the middle to the edges; ones added later - a bit after the move.
  const intro = untrack(() => introDelays(layout.nodes, layout.bounds));
  const delay = (id: string) => `${intro.get(id) ?? 0.15}s`;

  /** Bounds of the drawing (with labels), in layout units. */
  const bounds = $derived(layout.bounds);

  // -- Home: a full-width picture ------------------------------------------
  const viewBox = $derived.by(() => {
    const [x0, y0, x1, y1] = bounds;
    const pad = 12;
    // Few notes (a new vault): no larger than on the graph page, otherwise one
    // note fills the whole width. Spare room goes to the sides.
    const w = Math.max(x1 - x0 + 2 * pad, width / FIT_ZOOM);
    return `${(x0 + x1 - w) / 2} ${y0 - pad} ${w} ${y1 - y0 + 2 * pad}`;
  });
  // The figure shrinks in width (a phone): labels no smaller than 9 px on screen.
  let width = $state(0);
  let height = $state(0);
  const staticLabel = $derived.by(() => {
    const scale = width / Number(viewBox.split(" ")[2]);
    return scale > 0 ? Math.max(LABEL_SIZE, 9 / scale) : LABEL_SIZE;
  });


  // -- The graph page: zoom and pan ----------------------------------------
  let view = $state<View>({ x: 0, y: 0, k: 1 });
  /** The view is fitted and was not moved: the area changed size (window, forces panel) - fit again. */
  let snug = true;
  /** Fits the graph into the window (on a graph change and by the button). */
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
   * Brings back the core layout: nodes smoothly move to their places (eased,
   * no overshoot), the physics forgets the rearrangements.
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
    // The last frame is exactly the layout (no rounding error).
    frames.tween(GLIDE, (e) => (at = e < 1 ? glide(from, target, e) : target));
  }
  /** Shows a node: in the center of the window, no smaller than 1:1. */
  export function show(id: string) {
    const [x, y] = pos(id);
    const k = Math.max(view.k, 1);
    view = { k, x: width / 2 - x * k, y: height / 2 - y * k };
    snug = false;
    focused = id;
  }
  // A new graph (filter) or the first window measure: fit (on a graph change - smoothly, together with the nodes).
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
  /** A screen point in layout coordinates (with the graph page zoom or the picture's `viewBox`). */
  const world = (e: { clientX: number; clientY: number }): Point => {
    const m = layer?.getScreenCTM()?.inverse();
    if (!m) return [0, 0];
    const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(m);
    return [p.x, p.y];
  };

  // -- Physics: a dragged node pulls its neighbours ------------------------
  /** The picture frame (home, a note) is the `viewBox`; the graph page has no frame. */
  const frameRect = $derived.by((): Rect => {
    const [x0, y0, w, h] = viewBox.split(" ").map(Number) as [number, number, number, number];
    return [x0, y0, x0 + w, y0 + h];
  });
  const index = $derived(new Map(layout.nodes.map((n, i) => [n.id, i])));
  /** A book root is larger; a chapter is like a note. */
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
    // Node sizes by the labels on screen: they are larger than the core
    // assumed (a phone, zoomed out) and wider than the estimate by characters.
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

  /** Where the node is pulled (updated by pointer moves, applied in a frame). */
  let pull: { i: number; to: Point } | null = null;
  function run() {
    const p = sim!;
    frames.run(() => {
      if (pull) p.drag(pull.i, ...pull.to);
      // No motion: only the node itself, the neighbours stand.
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
    // The picture: the background goes to the page (scroll, selection), gestures only from a node.
    if (!interactive && (!id || pointers.size > 0)) return;
    try {
      svg!.setPointerCapture(e.pointerId);
    } catch {
      return; // the pointer is gone already: there will be no gesture
    }
    // The gesture goes to the graph: a mouse leaving its edge does not select page text.
    if (e.pointerType !== "touch") e.preventDefault();
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

  /** Releases the dragged node: it stays, the neighbours go a bit back. */
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
    // The node hit the frame and the pointer went further: remove the highlight.
    if (g?.kind === "node" && g.far && document.elementFromPoint(e.clientX, e.clientY)?.closest<SVGElement>(".graph-node")?.dataset.id !== g.id)
      focused = null;
    if (g?.kind === "node" && !g.far && e.type === "pointerup") {
      const node = graph.nodes.find((n) => n.id === g.id);
      if (node?.kind) open(g.id, e.ctrlKey || e.metaKey || e.button === 1);
    }
  }

  // The wheel with its own listener: a Svelte attribute one is passive, page scrolling cannot be cancelled.
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

  /** Opens a node: a note, a book or a book at a chapter. */
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
  /** Labels on screen no smaller than 8 px; very small ones hide (except books, found ones and the hovered one's neighbours). */
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
        <!-- the role is conditional (a link on the graph page), the check does not see it -->
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
          <!-- A book chapter is a hollow circle of the book's color: a part of the book, not a note. -->
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
