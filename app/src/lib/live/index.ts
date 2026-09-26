// Живые части заметки поверх готового HTML — реестр блоков: у каждого
// блока свой модуль (`plot.ts`, `frames.ts`, `graph.ts`) с селектором и
// функцией оживления. Общие для клиента и статического сайта (там — отдельной
// частью `static-live.js`, её грузит `static/main.ts` по `LIVE_SELECTOR`):
// без JS в HTML остаются кадр Typst, кадр по умолчанию и картинка графа.
//
// Новый живой блок = модуль с `LiveBlock` + селектор в `selectors.ts` +
// строка в `BLOCKS`.

import type { LiveBlock, OpenNote } from "./block";
import { frames } from "./frames";
import { graph } from "./graph";
import { plot } from "./plot";

export type { LiveBlock, LiveContext, OpenNote } from "./block";
export { LIVE_SELECTOR } from "./selectors";

/** Все живые блоки — по порядку оживления. */
export const BLOCKS: readonly LiveBlock[] = [plot, frames, graph];

/** Оживляет блоки внутри `root`; возвращает уборку (снять компоненты). */
export function mountLive(root: Element, open: OpenNote, blocks: readonly LiveBlock[] = BLOCKS): () => void {
  const cleanups: (() => void)[] = [];
  for (const block of blocks) {
    for (const el of root.querySelectorAll<HTMLElement>(block.selector)) {
      try {
        const cleanup = block.mount(el, { open });
        if (!cleanup) continue;
        cleanups.push(cleanup);
        // `data-live` прячет запасной вид (кадр Typst, картинку графа) — CSS.
        el.dataset.live = "";
      } catch (e) {
        console.warn(`${block.name}:`, e);
      }
    }
  }
  return () => {
    for (const c of cleanups) c();
  };
}
