import { expect, it } from "vitest";
import { vaultOpenMode } from "./vault-open";

const click = { button: 0, ctrlKey: false, metaKey: false };

it("vault open: the setting decides a plain click, unknown values mean this window", () => {
  expect(vaultOpenMode("this", click)).toBe("this");
  expect(vaultOpenMode("new", click)).toBe("new");
  expect(vaultOpenMode(undefined, click)).toBe("this");
  expect(vaultOpenMode("tab", click)).toBe("this");
  // Enter on a link has no gesture at all.
  expect(vaultOpenMode("new")).toBe("new");
  expect(vaultOpenMode("this")).toBe("this");
});

it("vault open: Ctrl, Cmd and middle click open a new window whatever the setting", () => {
  expect(vaultOpenMode("this", { ...click, ctrlKey: true })).toBe("new");
  expect(vaultOpenMode("this", { ...click, metaKey: true })).toBe("new");
  expect(vaultOpenMode("this", { ...click, button: 1 })).toBe("new");
  expect(vaultOpenMode("new", { ...click, ctrlKey: true })).toBe("new");
  expect(vaultOpenMode("this", { ...click, button: 2 })).toBe("this");
});

it("vault open: an explicit choice beats the setting and the gesture", () => {
  expect(vaultOpenMode("new", click, "this")).toBe("this");
  expect(vaultOpenMode("this", click, "new")).toBe("new");
  expect(vaultOpenMode("this", { ...click, ctrlKey: true }, "this")).toBe("this");
});
