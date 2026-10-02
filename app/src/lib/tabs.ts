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
 * Новая вкладка в фоне (Ctrl+щелчок): после активной и тех, что уже открыты
 * так же из неё (`opened`), — по порядку, как в браузере; активная та же.
 */
export function openBehind({ tabs, active }: TabList, url: string, opened: number): TabList {
  const at = Math.min(active + 1 + opened, tabs.length);
  return { tabs: [...tabs.slice(0, at), { url }, ...tabs.slice(at)], active };
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

/**
 * Убрать вкладки, для которых `drop` — истина (удалённая заметка): активной
 * остаётся та же вкладка, а если убрали её — ближайшая слева оставшаяся
 * (нет — справа). Не осталось ни одной — одна вкладка `fallback`.
 */
export function dropTabs({ tabs, active }: TabList, drop: (tab: Tab) => boolean, fallback: string): TabList & { activeDropped: boolean } {
  const keep = tabs.map((t) => !drop(t));
  const rest = tabs.filter((_, i) => keep[i]);
  if (!rest.length) return { tabs: [{ url: fallback }], active: 0, activeDropped: true };
  const activeDropped = !keep[active];
  // Оставшиеся левее активной (и она сама, если осталась): последняя из них
  // — новая активная; слева никого — первая справа.
  const left = keep.slice(0, active + 1).filter(Boolean).length;
  return { tabs: rest, active: clampActive(left - 1, rest.length), activeDropped };
}
