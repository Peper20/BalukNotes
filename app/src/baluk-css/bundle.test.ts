// @vitest-environment node
import { readdirSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { expect, it } from "vitest";
import { bundle, ENTRY } from "./bundle";

it("baluk.css: подключены все файлы блоков, по разу", async () => {
  const seen = new Set<string>();
  const css = await bundle(ENTRY, seen);
  const dir = dirname(ENTRY);
  const all = readdirSync(dir, { recursive: true, encoding: "utf8" })
    .filter((f) => f.endsWith(".css"))
    .map((f) => join(dir, f));
  expect([...seen].map((f) => relative(dir, f)).sort()).toEqual(all.map((f) => relative(dir, f)).sort());
  expect(css).not.toMatch(/^@import/m);
  expect(css).toContain(".k-note");
});
