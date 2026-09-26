// Сверка с ядром: индекс и ответы поиска ядра на запросы по tests/vault —
// эталон `tests/snapshots/search.json` (пишет `cargo test -p notes-core
// --test search_parity`). Клиент обязан ответить так же.
import { expect, it } from "vitest";
import reference from "../../../tests/snapshots/search.json";
import type { SearchDoc, SearchHit } from "./api";
import { search, snippet } from "./site-search";

const ref = reference as unknown as { docs: SearchDoc[]; cases: { query: string; limit: number; hits: SearchHit[] }[] };

it("эталон непустой", () => {
  expect(ref.docs.length).toBeGreaterThan(10);
  expect(ref.cases.some((c) => c.hits.length > 3)).toBe(true);
});

for (const c of ref.cases) {
  it(`как ядро: «${c.query}»`, () => {
    expect(search(ref.docs, c.query, c.limit)).toEqual(c.hits);
  });
}

it("фрагмент: регистр и «ё», обрезка вокруг первого совпадения", () => {
  const f = (s: { text: string; hit: boolean }[]) => s.map((x) => (x.hit ? `[${x.text}]` : x.text)).join("");
  const text = "Ёжик ищет ёлку. Потом ЕЖИК спит.";
  const fold = (s: string) => [...s.toLowerCase().replaceAll("ё", "е")];
  expect(f(snippet([...text], fold(text), [fold("ежик")]))).toBe("[Ёжик] ищет ёлку. Потом [ЕЖИК] спит.");
  const long = `${"слово ".repeat(40)} искомое ${"хвост ".repeat(60)}`;
  const s = f(snippet([...long], fold(long), [fold("искомое")]));
  expect(s.startsWith("…") && s.endsWith("…")).toBe(true);
  expect(s).toContain("[искомое]");
});
