// @vitest-environment node
import { readdirSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { expect, it } from "vitest";
import { bundle, ENTRY, lucideUrl } from "./bundle";

it("baluk.css: подключены все файлы блоков, по разу; значки подставлены", async () => {
  const seen = new Set<string>();
  const css = await bundle(ENTRY, seen);
  const dir = dirname(ENTRY);
  const all = readdirSync(dir, { recursive: true, encoding: "utf8" })
    .filter((f) => f.endsWith(".css"))
    .map((f) => join(dir, f));
  expect([...seen].map((f) => relative(dir, f)).sort()).toEqual(all.map((f) => relative(dir, f)).sort());
  expect(css).not.toMatch(/^@import|lucide\("[a-z]/m);
  expect(css).toContain("--k-icon-sun: url(\"data:image/svg+xml,");
});

it("значок Lucide — тот же SVG, что был переписан руками", async () => {
  expect(await lucideUrl("arrow-left", 1.75)).toBe(
    `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='1.75' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m12 19-7-7 7-7'/%3E%3Cpath d='M19 12H5'/%3E%3C/svg%3E")`,
  );
  await expect(lucideUrl("нет-такого", 2)).rejects.toThrow();
});
