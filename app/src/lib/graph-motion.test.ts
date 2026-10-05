import { describe, expect, it } from "vitest";
import type { Point } from "./graph-physics";
import { anyMoving, blendView, ease, Frames, glide, introDelays } from "./graph-motion";

/** Frames on command: `tick(now)` is the next frame. */
function manualFrames() {
  let queued: FrameRequestCallback | null = null;
  let id = 0;
  const frames = new Frames(
    (cb) => {
      queued = cb;
      return ++id;
    },
    () => (queued = null),
  );
  const tick = (now: number) => {
    const cb = queued;
    queued = null;
    cb?.(now);
    return cb != null;
  };
  return { frames, tick };
}

describe("graph motion", () => {
  it("easing: from 0 to 1 without overshoot", () => {
    expect(ease(0)).toBe(0);
    expect(ease(1)).toBe(1);
    let prev = 0;
    for (let t = 0.05; t <= 1; t += 0.05) {
      expect(ease(t)).toBeGreaterThanOrEqual(prev);
      expect(ease(t)).toBeLessThanOrEqual(1);
      prev = ease(t);
    }
  });

  it("moving: shared nodes move, new ones are in place at once", () => {
    const from = new Map<string, Point>([
      ["a", [0, 0]],
      ["gone", [5, 5]],
    ]);
    const to = new Map<string, Point>([
      ["a", [10, 20]],
      ["new", [3, 3]],
    ]);
    expect(anyMoving(from, to)).toBe(true);
    expect(anyMoving(new Map(), to)).toBe(false);
    const half = glide(from, to, 0.5);
    expect([...half]).toEqual([
      ["a", [5, 10]],
      ["new", [3, 3]],
    ]);
    expect(glide(from, to, 1)).toEqual(to);
    // The end of the way is exactly the target, no rounding error in the last digit.
    const [f, t] = [new Map<string, Point>([["a", [0.1, 300.7]]]), new Map<string, Point>([["a", [32.97356778396921, 20.8423722023761]]])];
    expect(glide(f, t, 1).get("a")).toStrictEqual(t.get("a"));
  });

  it("the view by the part of the way", () => {
    expect(blendView({ x: 0, y: 0, k: 1 }, { x: 10, y: -10, k: 2 }, 0.5)).toEqual({ x: 5, y: -5, k: 1.5 });
  });

  it("appearing: from the middle to the edges", () => {
    const d = introDelays(
      [
        { id: "mid", x: 50, y: 50 },
        { id: "edge", x: 100, y: 100 },
      ],
      [0, 0, 100, 100],
    );
    expect(d.get("mid")).toBe(0);
    expect(d.get("edge")).toBeCloseTo(0.35);
  });

  it("frames: a new loop cancels the previous one; a transition ends at e = 1", () => {
    const { frames, tick } = manualFrames();
    const seen: number[] = [];
    frames.run(() => {
      seen.push(-1);
      return true;
    });
    frames.tween(100, (e) => seen.push(e), 0);
    expect(frames.active).toBe(true);
    tick(50);
    tick(100);
    expect(tick(150)).toBe(false);
    expect(seen).toEqual([ease(0.5), 1]);
    expect(frames.active).toBe(false);
  });

  it("transition: a frame stamped before the start stays in place, not a step back", () => {
    const { frames, tick } = manualFrames();
    const seen: number[] = [];
    frames.tween(100, (e) => seen.push(e), 100);
    tick(90);
    expect(seen).toEqual([0]);
  });
});
