// Живые части заметки поверх готового HTML: интерактивные рисунки
// (`div.k-plot`), кадры (`div.k-frames`) и граф хранилища (`div.k-graph`).
// Общие для клиента и статического сайта (`src/static.ts`): без JS в HTML
// остаются кадр Typst, кадр по умолчанию и картинка графа.

import { mount, unmount } from "svelte";
import Frames from "../components/Frames.svelte";
import Graph from "../components/Graph.svelte";
import Plot from "../components/Plot.svelte";
import type { GraphLayout } from "./api";
import { parseFrames } from "./frames";
import { formulasOk, readSpec } from "./plot/spec";

/** Открыть заметку по щелчку на узле графа (в клиенте — без перезагрузки, на сайте — ссылкой). */
export type OpenNote = (id: string, newTab: boolean) => void;

/** Оживляет рисунки внутри `root`; возвращает уборку (снять компоненты). */
export function mountLive(root: Element, open: OpenNote): () => void {
  const mounted: ReturnType<typeof mount>[] = [];

  // Интерактивный рисунок встаёт рядом с кадром Typst, кадр прячет CSS
  // (`[data-live]`). Формула не разобралась — остаётся кадр.
  for (const el of root.querySelectorAll<HTMLElement>(".k-plot[data-k-plot]")) {
    const spec = readSpec(el);
    if (!spec || !formulasOk(spec)) continue;
    try {
      mounted.push(mount(Plot, { target: el, props: { spec } }));
      el.dataset.live = "";
    } catch (e) {
      console.warn("интерактивный рисунок:", e);
    }
  }

  // Кадры уже в разметке; добавляются ползунок и «▶», `data-live` включает
  // показ текущего кадра вместо кадра по умолчанию.
  for (const el of root.querySelectorAll<HTMLElement>(".k-frames[data-k-frames]")) {
    const spec = parseFrames(el.dataset.kFrames);
    const items = [...el.querySelectorAll<HTMLElement>(":scope > .k-frames-stack > .k-frames-item")];
    if (!spec || items.length !== spec.count) continue;
    try {
      mounted.push(mount(Frames, { target: el, props: { root: el, items, spec } }));
      el.dataset.live = "";
    } catch (e) {
      console.warn("кадры:", e);
    }
  }

  // Граф хранилища (`#vault-graph`): те же координаты, что у картинки Typst,
  // плюс наведение и переход к заметке. Картинку прячет CSS (`[data-live]`).
  for (const el of root.querySelectorAll<HTMLElement>(".k-graph[data-k-graph]")) {
    try {
      const layout = JSON.parse(el.dataset.kGraph ?? "") as GraphLayout;
      if (!Array.isArray(layout.nodes) || !layout.nodes.length) continue;
      const target = el.appendChild(Object.assign(document.createElement("div"), { className: "k-graph-live" }));
      mounted.push(mount(Graph, { target, props: { layout, onopen: open } }));
      el.dataset.live = "";
    } catch (e) {
      console.warn("граф хранилища:", e);
    }
  }

  return () => {
    for (const m of mounted) void unmount(m);
  };
}
