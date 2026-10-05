// Live parts of a note on top of the ready HTML, a registry of blocks: each
// block has its module (`plot.ts`, `frames.ts`, `graph.ts`) with a selector
// and an activation function. Without JS the HTML keeps the Typst frame, the
// default frame and the graph picture.
//
// A new live block = a module with `LiveBlock` + a selector in
// `selectors.ts` + a line in `BLOCKS`.

import type { LiveBlock, OpenNote } from "./block";
import { frames } from "./frames";
import { graph } from "./graph";
import { plot } from "./plot";

export type { LiveBlock, LiveContext, OpenNote } from "./block";
export { LIVE_SELECTOR } from "./selectors";

/** All live blocks, in activation order. */
export const BLOCKS: readonly LiveBlock[] = [plot, frames, graph];

/** Activates the blocks inside `root`; returns the cleanup (unmounts the components). */
export function mountLive(root: Element, open: OpenNote, blocks: readonly LiveBlock[] = BLOCKS): () => void {
  const cleanups: (() => void)[] = [];
  for (const block of blocks) {
    for (const el of root.querySelectorAll<HTMLElement>(block.selector)) {
      try {
        const cleanup = block.mount(el, { open });
        if (!cleanup) continue;
        cleanups.push(cleanup);
        // `data-live` hides the fallback look (the Typst frame, the graph picture) via CSS.
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
