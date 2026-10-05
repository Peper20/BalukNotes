import { expect, it } from "vitest";
import { combo } from "./keys";

const ev = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);

it("shortcuts go by the physical key, the layout does not matter", () => {
  const r = combo("KeyR");
  expect(r.label).toBe("R");
  expect(r.test(ev({ key: "к", code: "KeyR" }))).toBe(true);
  expect(r.test(ev({ key: "r", code: "KeyR", ctrlKey: true }))).toBe(false);
  const f = combo("Ctrl+Shift+KeyF");
  expect(f.label).toBe("Ctrl+Shift+F");
  expect(f.test(ev({ key: "F", code: "KeyF", ctrlKey: true, shiftKey: true }))).toBe(true);
  expect(f.test(ev({ key: "f", code: "KeyF", ctrlKey: true }))).toBe(false);
  expect(combo("BracketRight").label).toBe("]");
});

it("\"?\" goes by the character, with any Shift", () => {
  const q = combo("?");
  expect(q.test(ev({ key: "?", code: "Slash", shiftKey: true }))).toBe(true);
  expect(q.test(ev({ key: "?", code: "Digit7", shiftKey: true }))).toBe(true);
});
