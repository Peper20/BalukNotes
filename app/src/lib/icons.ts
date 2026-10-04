// Свои значки - в стиле lucide (24x24, контур), рисуются его <Icon iconNode>.

import type { LucideIconNode } from "@lucide/svelte";

/** Граф: четыре вершины, связи не заходят внутрь кружков. */
export const graphIcon: LucideIconNode[] = [
  ["path", { d: "M7.98 5.68L14.52 4.82 M6.82 8.12L9.18 11.88 M15.59 6.56L11.91 11.94 M12.60 15.36L16.90 18.14" }],
  ["circle", { cx: "5.5", cy: "6", r: "2.5" }],
  ["circle", { cx: "17", cy: "4.5", r: "2.5" }],
  ["circle", { cx: "10.5", cy: "14", r: "2.5" }],
  ["circle", { cx: "19", cy: "19.5", r: "2.5" }],
];
