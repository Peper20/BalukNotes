import { describe, expect, it } from "vitest";
import { extentOf, GRID_FROM, nearPairs, Physics, type PhysicsNode, type Rect } from "./graph-physics";

// A chain a - b - c - d horizontally and e without links aside.
const node = (x: number, y: number, name = "узел"): PhysicsNode => ({ x, y, extent: extentOf(6, name, 11, 2) });
const nodes = () => [node(0, 0), node(70, 0), node(140, 0), node(210, 0), node(100, 120)];
const edges: [number, number][] = [
  [0, 1],
  [1, 2],
  [2, 3],
];

/** Steps until it settles (at most `limit` steps); returns the number of steps. */
function settle(p: Physics, limit = 2000): number {
  for (let i = 1; i <= limit; i++) if (p.settled(p.step())) return i;
  return limit;
}

describe("graph physics", () => {
  it("the core layout is rest: an untouched graph does not move", () => {
    const p = new Physics(nodes(), edges);
    const before = p.pos.slice();
    expect(p.settled(p.step())).toBe(true);
    expect([...p.pos]).toEqual([...before]);
  });

  it("neighbours follow the node, far ones weaker, an unlinked one stands", () => {
    const p = new Physics(nodes(), edges);
    for (let i = 0; i < 60; i++) {
      p.drag(0, -80, 40);
      p.step();
    }
    const dx = (i: number) => p.at(i)[0] - nodes()[i]!.x;
    expect(dx(1)).toBeLessThan(-10);
    expect(dx(2)).toBeLessThan(0);
    expect(Math.abs(dx(2))).toBeLessThan(Math.abs(dx(1)));
    expect(Math.abs(dx(3))).toBeLessThan(Math.abs(dx(2)));
    expect(p.at(4)).toEqual([100, 120]);
  });

  it("on release the node stands, the neighbours smoothly go a bit back and stop", () => {
    const p = new Physics(nodes(), edges);
    for (let i = 0; i < 60; i++) {
      p.drag(3, 300, 60);
      p.step();
    }
    const [x2] = p.at(2);
    expect(x2).toBeGreaterThan(160);
    const drop = p.at(3);
    p.release();
    const xs: number[] = [];
    let steps = 0;
    for (; steps < 300; steps++) {
      const f = p.step();
      xs.push(p.at(2)[0]);
      if (p.settled(f)) break;
    }
    expect(steps).toBeLessThan(120); // stopped within a couple of seconds
    const end = xs.at(-1)!;
    // went about a quarter of the way to the old place (140), no overshoot
    expect((x2 - end) / (x2 - 140)).toBeGreaterThan(0.1);
    expect((x2 - end) / (x2 - 140)).toBeLessThan(0.4);
    expect(Math.min(...xs)).toBeGreaterThan(end - 0.5);
    expect(p.at(3)).toEqual(drop); // the dropped one did not sway
  });

  it("the neighbours' response is tunable: they pull harder, do not return", () => {
    const pulled = (pull: number) => {
      const p = new Physics(nodes(), edges);
      p.response = { pull, back: 0 };
      for (let i = 0; i < 60; i++) {
        p.drag(0, -80, 40);
        p.step();
      }
      return p;
    };
    const shift = (p: Physics) => Math.abs(p.at(1)[0] - 70);
    expect(shift(pulled(0))).toBe(0); // edges do not pull
    expect(shift(pulled(3))).toBeGreaterThan(shift(pulled(1)));
    const p = pulled(1);
    const at = p.at(1);
    p.release();
    settle(p);
    expect(p.at(1)[0]).toBeCloseTo(at[0], 0); // did not go back
  });

  it("a node held in place: the neighbours come without swaying", () => {
    const p = new Physics(nodes(), edges);
    const xs: number[] = [];
    for (let i = 0; i < 200; i++) {
      p.drag(0, -Math.min(i, 20) * 8, 0); // pulled left and held
      p.step();
      xs.push(p.at(1)[0]);
    }
    const end = xs.at(-1)!;
    // the neighbour moves left and does not overshoot its final place
    expect(Math.min(...xs.slice(20))).toBeGreaterThan(end - 0.5);
  });

  it("deterministic: the same actions give the same coordinates", () => {
    const run = () => {
      const p = new Physics(nodes(), edges);
      for (let i = 0; i < 40; i++) {
        p.drag(1, 70 + i * 3, i * 2);
        p.step();
      }
      p.release();
      settle(p);
      return [...p.pos];
    };
    expect(run()).toEqual(run());
  });

  it("overlapping nodes are pushed apart", () => {
    const p = new Physics(nodes(), edges);
    p.drag(4, 140, 10); // onto c
    for (let i = 0; i < 100; i++) p.step();
    const [cx, cy] = p.at(2);
    const box = extentOf(6, "узел", 11, 2);
    const apart = Math.abs(cx - 140) >= 2 * box.half - 1 || Math.abs(cy - 10) >= box.top + box.bottom - 1;
    expect(apart).toBe(true);
  });

  it("larger sizes on screen: rest is still rest, and overlapping nodes are pushed apart by them", () => {
    const p = new Physics(nodes(), edges);
    const big = extentOf(6, "очень длинная подпись узла", 14, 2);
    p.setExtents(nodes().map(() => big));
    const before = p.pos.slice();
    expect(p.settled(p.step())).toBe(true);
    expect([...p.pos]).toEqual([...before]);
    p.drag(4, 100, 0); // between b and c
    for (let i = 0; i < 100; i++) p.step();
    expect(Math.abs(p.at(1)[0] - 100) > 12.76 * 2 || Math.abs(p.at(2)[0] - 100) > 12.76 * 2).toBe(true);
  });

  it("frame: neither the node nor its neighbours can be dragged past the edge", () => {
    const frame: Rect = [-20, -20, 230, 150];
    const p = new Physics(nodes(), edges, frame);
    const inside = () => {
      for (let i = 0; i < p.n; i++) {
        const [x, y] = p.at(i);
        const e = extentOf(6, "узел", 11, 2);
        expect(x - e.half).toBeGreaterThanOrEqual(frame[0] - 1e-9);
        expect(x + e.half).toBeLessThanOrEqual(frame[2] + 1e-9);
        expect(y - e.top).toBeGreaterThanOrEqual(frame[1] - 1e-9);
        expect(y + e.bottom).toBeLessThanOrEqual(frame[3] + 1e-9);
      }
    };
    for (let i = 0; i < 200; i++) {
      p.drag(0, -1000 - i, -1000);
      p.step();
      inside();
    }
    expect(p.at(1)[0]).toBeLessThan(68); // the neighbour still came closer (the node hit the frame corner)
    p.release();
    settle(p);
    inside();
  });

  it("at the frame edge overlapping nodes go apart along the edge", () => {
    // A node at the top edge with another slipped under it: no room upwards, so sideways.
    const p = new Physics([node(0, 0), node(100, 0)], [], [-200, -6, 300, 200]);
    for (let i = 0; i < 100; i++) {
      p.drag(1, 0, 12);
      p.step();
    }
    const e = extentOf(6, "узел", 11, 2);
    expect(p.at(0)[1]).toBeCloseTo(0, 6);
    expect(Math.abs(p.at(0)[0])).toBeGreaterThanOrEqual(2 * e.half - 0.5);
  });

  it("the frame does not move a node standing at home outside it (a large label)", () => {
    const p = new Physics(nodes(), edges, [0, 0, 100, 100]);
    expect(p.settled(p.step())).toBe(true);
    expect(p.at(3)).toEqual([210, 0]);
  });
});

/** A big graph as in the core: nodes on a grid with scatter, labels of different length, ~1.5 edges per node. */
function bigGraph(n: number): { nodes: PhysicsNode[]; edges: [number, number][] } {
  let seed = 1;
  const rnd = () => ((seed = (seed * 1103515245 + 12345) % 2 ** 31) / 2 ** 31);
  const side = Math.ceil(Math.sqrt(n));
  const nodes = Array.from({ length: n }, (_, i) => ({
    x: (i % side) * 70 + rnd() * 30,
    y: Math.floor(i / side) * 45 + rnd() * 20,
    extent: extentOf(4 + rnd() * 6, "узел".repeat(1 + Math.floor(rnd() * 3)), 11, 2),
  }));
  const edges = Array.from({ length: Math.round(n * 1.5) }, (): [number, number] => [Math.floor(rnd() * n), Math.floor(rnd() * n)]);
  return { nodes, edges };
}

describe("physics of a big graph", () => {
  it("the grid finds the same overlapping pairs in the same order", () => {
    const { nodes } = bigGraph(600);
    // A squeezed layout: many overlaps.
    const pos = new Float64Array(nodes.flatMap((p) => [p.x * 0.4, p.y * 0.4]));
    const extents = nodes.map((p) => p.extent);
    const overlapping = (grid: boolean) => {
      const out: string[] = [];
      nearPairs(
        pos,
        extents,
        (i, j) => {
          const [a, b] = [extents[i]!, extents[j]!];
          const dx = Math.abs(pos[2 * j]! - pos[2 * i]!);
          const dy = pos[2 * j + 1]! - pos[2 * i + 1]!;
          const h = dy >= 0 ? a.bottom + b.top : a.top + b.bottom;
          if (a.half + b.half > dx && h > Math.abs(dy)) out.push(`${i}-${j}`);
        },
        grid,
      );
      return out;
    };
    const all = overlapping(false);
    expect(all.length).toBeGreaterThan(100);
    expect(overlapping(true)).toEqual(all);
  });

  it(`1000 nodes: a step under 4 ms (grid from ${GRID_FROM} nodes)`, () => {
    const { nodes, edges } = bigGraph(1000);
    const p = new Physics(nodes, edges);
    p.setExtents(nodes.map((n) => n.extent));
    const times: number[] = [];
    for (let i = 0; i < 80; i++) {
      p.drag(500, nodes[500]!.x + i * 4, nodes[500]!.y + i * 2);
      const t = performance.now();
      p.step();
      times.push(performance.now() - t);
    }
    times.sort((a, b) => a - b);
    const median = times[times.length >> 1]!;
    expect(median).toBeLessThan(4);
  });
});
