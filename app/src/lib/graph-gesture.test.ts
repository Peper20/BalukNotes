import { describe, expect, it } from "vitest";
import { DRAG_START, Pointers } from "./graph-gesture";

const view = { x: 0, y: 0, k: 1 };

describe("graph gestures", () => {
  it("background: pans after the pointer", () => {
    const p = new Pointers();
    expect(p.down(1, [10, 10], view, null)).toBe("pan");
    expect(p.move(1, [30, 5])).toEqual({ kind: "view", view: { x: 20, y: -5, k: 1 }, pan: true });
    expect(p.up(1, view)).toMatchObject({ kind: "end", gesture: { kind: "pan" } });
    expect(p.gesture).toBeNull();
  });

  it("node: a small move is still a click, beyond - a drag", () => {
    const p = new Pointers();
    expect(p.down(1, [100, 100], view, { id: "a", grab: [1, 2] })).toBe("node");
    expect(p.move(1, [100 + DRAG_START - 1, 100])).toEqual({ kind: "none" });
    expect(p.move(1, [120, 100])).toEqual({ kind: "drag", id: "a", grab: [1, 2], first: true });
    expect(p.move(1, [101, 100])).toEqual({ kind: "drag", id: "a", grab: [1, 2], first: false });
    expect(p.up(1, view)).toEqual({ kind: "end", gesture: { kind: "node", id: "a", start: [100, 100], grab: [1, 2], far: true } });
  });

  it("a click on a node is a gesture without a drag", () => {
    const p = new Pointers();
    p.down(1, [0, 0], view, { id: "a", grab: [0, 0] });
    p.move(1, [1, 1]);
    expect(p.up(1, view)).toMatchObject({ kind: "end", gesture: { kind: "node", far: false } });
  });

  it("the node is no longer in the graph: do not drag", () => {
    const p = new Pointers();
    p.down(1, [0, 0], view, { id: "a", grab: [0, 0] });
    expect(p.move(1, [50, 0], () => false)).toEqual({ kind: "none" });
    expect(p.up(1, view)).toMatchObject({ gesture: { far: false } });
  });

  it("two fingers: zoom at the middle, pan after it; one released - pan", () => {
    const p = new Pointers();
    p.down(1, [0, 0], view, { id: "a", grab: [0, 0] });
    expect(p.down(2, [100, 0], view, null)).toBe("pinch");
    // Spread twice symmetrically: the middle is the same, zoom ×2 at (50, 0).
    const zoom = p.move(2, [150, 0]);
    expect(p.move(1, [-50, 0])).toEqual({ kind: "view", view: { x: -50, y: 0, k: 2 }, pan: false });
    expect(zoom.kind).toBe("view");
    const now = { x: -50, y: 0, k: 2 };
    expect(p.up(2, now)).toEqual({ kind: "continue" });
    expect(p.gesture).toEqual({ kind: "pan", start: [-50, 0], view: now });
    expect(p.move(1, [-40, 5])).toEqual({ kind: "view", view: { x: -40, y: 5, k: 2 }, pan: true });
  });

  it("a foreign pointer and a third finger are not a gesture", () => {
    const p = new Pointers();
    expect(p.move(9, [1, 1])).toEqual({ kind: "none" });
    expect(p.up(9, view)).toEqual({ kind: "none" });
    p.down(1, [0, 0], view, null);
    p.down(2, [10, 0], view, null);
    expect(p.down(3, [20, 0], view, null)).toBeNull();
    expect(p.up(3, view)).toEqual({ kind: "continue" });
  });
});
