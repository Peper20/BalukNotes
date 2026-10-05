import { expect, it, vi } from "vitest";
import { LIVE_SELECTOR, mountLive, type LiveBlock } from ".";

it("registry: a block activates its elements, data-live only on activated ones", () => {
  const root = document.createElement("div");
  root.innerHTML = `<p class="x" data-ok></p><p class="x"></p><p class="x" data-fail></p><p class="y"></p>`;
  const cleaned: string[] = [];
  const block: LiveBlock = {
    name: "probe",
    selector: ".x",
    mount(el) {
      if (el.hasAttribute("data-fail")) throw new Error("broken");
      if (!el.hasAttribute("data-ok")) return null;
      return () => cleaned.push("x");
    },
  };
  const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
  const unmount = mountLive(root, () => {}, [block]);
  const live = [...root.querySelectorAll(".x")].map((el) => el.hasAttribute("data-live"));
  expect(live).toEqual([true, false, false]);
  expect(warn).toHaveBeenCalledOnce();
  unmount();
  expect(cleaned).toEqual(["x"]);
  warn.mockRestore();
});

it("the live block selector finds all three kinds", () => {
  const root = document.createElement("div");
  root.innerHTML = `<div class="k-plot" data-k-plot></div><div class="k-frames" data-k-frames></div><div class="k-graph" data-k-graph></div><div class="k-frame"></div>`;
  expect(root.querySelectorAll(LIVE_SELECTOR)).toHaveLength(3);
});
