// Вкладки клиента — чистая логика (состояние — state/tabs.svelte.ts).

/** Вкладка — адрес (путь и якорь) внутри клиента. */
export interface Tab {
  url: string;
}

export interface TabList {
  tabs: Tab[];
  active: number;
}

/** Номер активной вкладки в пределах списка. */
export const clampActive = (active: number, count: number): number => Math.min(Math.max(active, 0), count - 1);

/** Новая вкладка сразу после активной; она же становится активной. */
export function openAfter({ tabs, active }: TabList, url: string): TabList {
  return { tabs: [...tabs.slice(0, active + 1), { url }, ...tabs.slice(active + 1)], active: active + 1 };
}

/**
 * Закрыть вкладку `i` (последнюю закрыть нельзя — null): активной остаётся
 * та же вкладка, а если закрыли её — соседняя слева (крайняя — справа).
 */
export function closeAt({ tabs, active }: TabList, i: number): (TabList & { wasActive: boolean }) | null {
  if (i < 0 || i >= tabs.length || tabs.length === 1) return null;
  const rest = tabs.filter((_, j) => j !== i);
  const next = i < active || active >= rest.length ? active - 1 : active;
  return { tabs: rest, active: next, wasActive: i === active };
}

/** Соседняя вкладка по кругу: `delta` = ±1. */
export const cycle = (active: number, count: number, delta: number): number => (active + delta + count) % count;
