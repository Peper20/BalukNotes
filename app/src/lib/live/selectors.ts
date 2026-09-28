// Селекторы живых блоков — отдельно от их кода: узнать, есть ли на странице
// живые блоки, можно, не загружая сами блоки.

export const SELECTORS = {
  /** Интерактивный рисунок (`baluk/plots.typ`). */
  plot: ".k-plot[data-k-plot]",
  /** Кадры (`baluk/frames.typ`). */
  frames: ".k-frames[data-k-frames]",
  /** Граф хранилища (`#vault-graph`, `baluk/graph.typ`). */
  graph: ".k-graph[data-k-graph]",
} as const;

/** Любой живой блок. */
export const LIVE_SELECTOR = Object.values(SELECTORS).join(", ");
