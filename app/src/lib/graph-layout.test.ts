import { expect, it } from "vitest";
import { layout } from "./graph-layout";

const ids = ["a", "b", "c", "d", "одинокий-1", "одинокий-2", "одинокий-3"];
const edges = [
  { from: "a", to: "b" },
  { from: "b", to: "c" },
  { from: "c", to: "a" },
  { from: "a", to: "d" },
];

it("детерминирована", () => {
  expect([...layout(ids, edges)]).toEqual([...layout(ids, edges)]);
});

it("несвязанные узлы не улетают далеко от связной части", () => {
  const pos = layout(ids, edges);
  const dist = (p: string, q: string) => Math.hypot(pos.get(p)![0] - pos.get(q)![0], pos.get(p)![1] - pos.get(q)![1]);
  const cluster = Math.max(dist("a", "b"), dist("b", "c"), dist("a", "d"));
  for (const lone of ["одинокий-1", "одинокий-2", "одинокий-3"]) expect(dist(lone, "a")).toBeLessThan(cluster * 8);
  expect(dist("a", "b")).toBeGreaterThan(20);
});
