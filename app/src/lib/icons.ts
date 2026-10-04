// Свои значки - в стиле lucide (24x24, контур), рисуются его <Icon iconNode>.

import type { LucideIconNode } from "@lucide/svelte";

/** Граф: звезда - центр и три вершины; связи не заходят внутрь кружков. */
export const graphIcon: LucideIconNode[] = [
  ["path", { d: "M12.00 6.50L12.00 10.50 M6.52 17.02L9.98 14.48 M17.48 17.02L14.02 14.48" }],
  ["circle", { cx: "12", cy: "4", r: "2.5" }],
  ["circle", { cx: "4.5", cy: "18.5", r: "2.5" }],
  ["circle", { cx: "19.5", cy: "18.5", r: "2.5" }],
  ["circle", { cx: "12", cy: "13", r: "2.5" }],
];
