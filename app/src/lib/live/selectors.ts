// Selectors of live blocks, apart from their code: one can learn whether
// the page has live blocks without loading the blocks themselves.

export const SELECTORS = {
  /** An interactive figure (`baluk/plots.typ`). */
  plot: ".k-plot[data-k-plot]",
  /** Frames (`baluk/frames.typ`). */
  frames: ".k-frames[data-k-frames]",
  /** The vault graph (`#vault-graph`, `baluk/graph.typ`). */
  graph: ".k-graph[data-k-graph]",
} as const;

/** Any live block. */
export const LIVE_SELECTOR = Object.values(SELECTORS).join(", ");
