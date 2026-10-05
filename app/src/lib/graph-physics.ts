// The live graph: physics while something is done to the graph. The core
// layout (`notes-core::vault_graph`) is the state of rest: each node holds
// to its place ("home") with a weak spring, edges are springs as long as in
// the layout, overlapping node rectangles with labels are pushed apart (as
// in the core). At rest all forces are zero: an untouched graph does not
// move, PDF and the app show one picture.
//
// Drag a node and its neighbours follow it along the edges, their
// neighbours weaker; strong damping, no swaying. Release it and the node
// stays where it was dropped, while the neighbours smoothly go a bit back
// (`RETURN` of the way to their old places) and stop: a full return by the
// springs "shook" the graph, and no motion at all made it freeze dead. The
// frame (`frame`): nodes with their labels do not leave it.
//
// Pushing apart compares pairs of nodes: in a big graph (from `GRID_FROM`
// nodes) only grid neighbours (`nearPairs`), otherwise a frame would take
// tens of milliseconds on a thousand nodes. The order of pairs is the same
// as when trying all.

export type Point = [number, number];
/** A rectangle `[x0, y0, x1, y1]`. */
export type Rect = [number, number, number, number];

/** Node size around its center: half width (circle or label), top and bottom (with the label). */
export interface Extent {
  half: number;
  top: number;
  bottom: number;
}

export interface PhysicsNode {
  x: number;
  y: number;
  /** Size for pushing apart, as in the core (the label in the layout font size), otherwise rest would not be rest. */
  extent: Extent;
}

/** Stiffness of the home spring and of the edges, velocity damping per step. */
const HOME = 0.05;
const SPRING = 0.07;
const DAMPING = 0.3;
/** Which part of the way to the old place the neighbours go after a release. */
const RETURN = 0.25;

/**
 * Neighbours' response to dragging (graph settings): `pull` - how many times
 * stronger than usual the edges pull the neighbours after the node (0 - they
 * stay), `back` - which part of the way to their old places they go when
 * the node is released.
 */
export interface Drag {
  pull: number;
  back: number;
}
export const DRAG: Drag = { pull: 1, back: RETURN };

/** Shift (layout units per step) below which the graph is settled. */
const REST = 0.02;
/** From how many nodes the pairs for pushing apart come from a grid, not all. */
export const GRID_FROM = 200;

/**
 * Pairs of nodes `i < j` whose rectangles (`extents` around the points `pos`)
 * may overlap, in the order of trying all (by `i`, then by `j`). Up to
 * `GRID_FROM` nodes all pairs; beyond, a grid with the cell of the largest
 * rectangle: only nodes from neighbouring cells overlap.
 */
export function nearPairs(pos: Float64Array, extents: Extent[], visit: (i: number, j: number) => void, grid = extents.length >= GRID_FROM): void {
  const n = extents.length;
  if (!grid) {
    for (let i = 0; i < n; i++) for (let j = i + 1; j < n; j++) visit(i, j);
    return;
  }
  let [cw, ch] = [1e-6, 1e-6];
  for (const e of extents) {
    cw = Math.max(cw, 2 * e.half);
    ch = Math.max(ch, e.top + e.bottom);
  }
  const cell = new Int32Array(2 * n);
  const cells = new Map<number, number[]>();
  const key = (cx: number, cy: number) => cx * 1_000_003 + cy;
  for (let i = 0; i < n; i++) {
    const cx = Math.floor(pos[2 * i]! / cw);
    const cy = Math.floor(pos[2 * i + 1]! / ch);
    cell[2 * i] = cx;
    cell[2 * i + 1] = cy;
    const k = key(cx, cy);
    const list = cells.get(k);
    if (list) list.push(i);
    else cells.set(k, [i]);
  }
  const near: number[] = [];
  for (let i = 0; i < n; i++) {
    near.length = 0;
    const [cx, cy] = [cell[2 * i]!, cell[2 * i + 1]!];
    for (let dx = -1; dx <= 1; dx++) {
      for (let dy = -1; dy <= 1; dy++) {
        for (const j of cells.get(key(cx + dx, cy + dy)) ?? []) if (j > i) near.push(j);
      }
    }
    near.sort((a, b) => a - b);
    for (const j of near) visit(i, j);
  }
}

export class Physics {
  readonly n: number;
  readonly pos: Float64Array;
  private readonly vel: Float64Array;
  private readonly home: Float64Array;
  private extents: Extent[];
  private readonly links: [number, number, number][];
  private readonly lo: Float64Array;
  private readonly hi: Float64Array;
  private frame: Rect | null = null;
  private frameExtents: Extent[];
  /** The node under the pointer. */
  private held = -1;
  /** A dropped node: it stands while the neighbours settle (otherwise their springs would sway it). */
  private dropped = -1;
  /** Neighbours' response, can change on the fly. */
  response: Drag = DRAG;

  constructor(nodes: PhysicsNode[], edges: [number, number][], frame: Rect | null = null) {
    this.n = nodes.length;
    this.pos = new Float64Array(nodes.flatMap((p) => [p.x, p.y]));
    this.home = this.pos.slice();
    this.vel = new Float64Array(2 * this.n);
    this.extents = nodes.map((p) => p.extent);
    this.frameExtents = this.extents;
    this.links = edges.filter(([a, b]) => a !== b).map(([a, b]) => [a, b, this.span(a, b)]);
    this.lo = new Float64Array(2 * this.n);
    this.hi = new Float64Array(2 * this.n);
    this.setFrame(frame);
  }

  /**
   * Sizes for pushing apart, by the labels on screen (they can be larger
   * than the core assumed), but shrunk just enough for the homes not to
   * overlap: rest stays rest. Only nodes whose homes overlap are shrunk.
   */
  setExtents(extents: Extent[]) {
    const s = new Array<number>(this.n).fill(1);
    nearPairs(this.home, extents, (i, j) => {
      const [ei, ej] = [extents[i]!, extents[j]!];
      const dx = Math.abs(this.home[2 * j]! - this.home[2 * i]!);
      const dy = this.home[2 * j + 1]! - this.home[2 * i + 1]!;
      const w = ei.half + ej.half;
      const h = dy >= 0 ? ei.bottom + ej.top : ei.top + ej.bottom;
      if (w <= dx || h <= Math.abs(dy)) return;
      const f = Math.max(dx / w, Math.abs(dy) / h);
      s[i] = Math.min(s[i]!, f);
      s[j] = Math.min(s[j]!, f);
    });
    this.extents = extents.map((e, i) => ({ half: e.half * s[i]!, top: e.top * s[i]!, bottom: e.bottom * s[i]! }));
  }

  /** Distance between the homes of nodes. */
  private span(a: number, b: number): number {
    return Math.hypot(this.home[2 * a]! - this.home[2 * b]!, this.home[2 * a + 1]! - this.home[2 * b + 1]!);
  }

  /**
   * The frame nodes (with a label of size `extents`, by default as when
   * pushing apart) do not leave; `null` - no frame. A node whose home is
   * already outside the frame (the label on screen is larger than the core
   * assumed) may stay home: the frame widens to its home.
   */
  setFrame(frame: Rect | null, extents: Extent[] = this.extents) {
    this.frame = frame;
    this.frameExtents = extents;
    for (let i = 0; i < this.n; i++) {
      if (!frame) {
        this.lo.fill(-Infinity, 2 * i, 2 * i + 2);
        this.hi.fill(Infinity, 2 * i, 2 * i + 2);
        continue;
      }
      const e = extents[i]!;
      const [hx, hy] = [this.home[2 * i]!, this.home[2 * i + 1]!];
      this.lo[2 * i] = Math.min(frame[0] + e.half, hx);
      this.hi[2 * i] = Math.max(frame[2] - e.half, hx);
      this.lo[2 * i + 1] = Math.min(frame[1] + e.top, hy);
      this.hi[2 * i + 1] = Math.max(frame[3] - e.bottom, hy);
    }
  }

  at(i: number): Point {
    return [this.pos[2 * i]!, this.pos[2 * i + 1]!];
  }

  /** Holds node `i` at a point (within the frame). */
  drag(i: number, x: number, y: number) {
    this.held = i;
    this.dropped = -1;
    this.pos[2 * i] = clamp(x, this.lo[2 * i]!, this.hi[2 * i]!);
    this.pos[2 * i + 1] = clamp(y, this.lo[2 * i + 1]!, this.hi[2 * i + 1]!);
    this.vel[2 * i] = this.vel[2 * i + 1] = 0;
  }

  /** Releases the node: it stays, the others go a bit back to their old places (a new rest). */
  release() {
    const i = this.held;
    if (i < 0) return;
    this.held = -1;
    this.dropped = i;
    for (let k = 0; k < 2 * this.n; k++) {
      const back = k >> 1 === i ? 0 : this.response.back;
      this.home[k] = this.pos[k]! + (this.home[k]! - this.pos[k]!) * back;
    }
    for (const l of this.links) l[2] = this.span(l[0], l[1]);
    this.setFrame(this.frame, this.frameExtents);
  }

  get holding(): boolean {
    return this.held >= 0;
  }

  private fixed(i: number): boolean {
    return i === this.held || i === this.dropped;
  }


  /** One step; returns the largest node shift in the step, for [`settled`]. */
  step(): number {
    const { n, pos, vel, home, extents } = this;
    const before = pos.slice();
    const force = new Float64Array(2 * n);
    for (let i = 0; i < 2 * n; i++) force[i] = (home[i]! - pos[i]!) * HOME;
    const spring = SPRING * this.response.pull;
    for (const [a, b, len] of this.links) {
      const dx = pos[2 * b]! - pos[2 * a]!;
      const dy = pos[2 * b + 1]! - pos[2 * a + 1]!;
      const d = Math.max(Math.hypot(dx, dy), 1e-6);
      const f = ((d - len) / d) * spring;
      force[2 * a]! += dx * f;
      force[2 * a + 1]! += dy * f;
      force[2 * b]! -= dx * f;
      force[2 * b + 1]! -= dy * f;
    }
    for (let i = 0; i < n; i++) {
      if (this.fixed(i)) continue;
      for (const k of [2 * i, 2 * i + 1]) {
        vel[k] = (vel[k]! + force[k]!) * DAMPING;
        pos[k]! += vel[k]!;
      }
    }
    // Overlapping rectangles go apart along the axis with the smaller
    // overlap, at once (not by a force: springs would press nodes together).
    nearPairs(pos, extents, (i, j) => {
      const [ei, ej] = [extents[i]!, extents[j]!];
      const dx = pos[2 * j]! - pos[2 * i]!;
      const dy = pos[2 * j + 1]! - pos[2 * i + 1]!;
      const ox = ei.half + ej.half - Math.abs(dx);
      if (ox <= 0) return;
      const oy = (dy >= 0 ? ei.bottom + ej.top : ei.top + ej.bottom) - Math.abs(dy);
      if (oy <= 0) return;
      const [fi, fj] = [this.fixed(i), this.fixed(j)];
      // Along the axis with the smaller overlap; if it hits the frame there, along the other.
      const axes: [number, number, number][] = [
        [0, dx, ox],
        [1, dy, oy],
      ];
      if (oy < ox) axes.reverse();
      for (const [k, d, o] of axes) {
        const dir = d < 0 ? -1 : 1;
        const [si, sj] = fi ? [0, o] : fj ? [o, 0] : [o / 2, o / 2];
        const mi = this.room(i, k, -dir * si);
        const mj = this.room(j, k, dir * sj);
        if (Math.abs(mi) + Math.abs(mj) < o / 2 && k === axes[0]![0]) continue;
        pos[2 * i + k]! += mi;
        pos[2 * j + k]! += mj;
        break;
      }
    });
    let fastest = 0;
    for (let i = 0; i < n; i++) {
      if (this.fixed(i)) continue;
      for (const k of [2 * i, 2 * i + 1]) {
        pos[k] = clamp(pos[k]!, this.lo[k]!, this.hi[k]!);
        vel[k] = pos[k]! - before[k]!;
        fastest = Math.max(fastest, Math.abs(vel[k]!));
      }
    }
    return fastest;
  }

  /** How far node `i` can move along axis `k` by `delta` (a held one - not at all, the frame - up to its edge). */
  private room(i: number, k: number, delta: number): number {
    if (this.fixed(i) || delta === 0) return 0;
    const at = this.pos[2 * i + k]!;
    return clamp(at + delta, this.lo[2 * i + k]!, this.hi[2 * i + k]!) - at;
  }

  /** Settled: nobody is held and all nearly stand. */
  settled(fastest: number): boolean {
    const done = this.held < 0 && fastest < REST;
    if (done) this.dropped = -1;
    return done;
  }
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** Label width by the number of characters, as in the core (`vault_graph::label_width`). */
export const labelWidth = (text: string, size: number) => [...text].length * size * 0.58;

/** Size of a node with the label under the circle (`vault_graph::box_rect`). */
export function extentOf(r: number, name: string, size: number, gap: number): Extent {
  return { half: Math.max(r, labelWidth(name, size) / 2), top: r, bottom: r + gap + size * 1.2 };
}
