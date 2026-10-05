import { expect, it } from "vitest";
import { placeFromHistory, pushRecent, rememberPlace } from "./places";

it("places: the fresh one last, extra ones forgotten", () => {
  let p = {};
  for (const id of ["a", "b", "c"]) p = rememberPlace(p, id, { y: 1, chapter: null }, 2);
  expect(Object.keys(p)).toEqual(["b", "c"]);
  p = rememberPlace(p, "b", { y: 5, chapter: 2 }, 2);
  expect(Object.keys(p)).toEqual(["c", "b"]);
  expect(p).toMatchObject({ b: { y: 5, chapter: 2 } });
});

it("recent: no repeats, within the limit", () => {
  expect(pushRecent(["a", "b", "c"], "b", 3)).toEqual(["b", "a", "c"]);
  expect(pushRecent(["a", "b", "c"], "d", 3)).toEqual(["d", "a", "b"]);
});

it("the place from the history", () => {
  expect(placeFromHistory({ y: 10 })).toEqual({ y: 10, chapter: null });
  expect(placeFromHistory({ y: 3, chapter: 1 })).toEqual({ y: 3, chapter: 1 });
  expect(placeFromHistory(null)).toBeNull();
  expect(placeFromHistory({})).toBeNull();
});
