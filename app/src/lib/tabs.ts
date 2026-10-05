// Client tabs, pure logic (the state is state/tabs.svelte.ts).

/** A tab is an address (path and anchor) inside the client. */
export interface Tab {
  url: string;
}

export interface TabList {
  tabs: Tab[];
  active: number;
}

/** Index of the active tab within the list. */
export const clampActive = (active: number, count: number): number => Math.min(Math.max(active, 0), count - 1);

/** A new tab right after the active one; it becomes active. */
export function openAfter({ tabs, active }: TabList, url: string): TabList {
  return { tabs: [...tabs.slice(0, active + 1), { url }, ...tabs.slice(active + 1)], active: active + 1 };
}

/**
 * A new background tab (Ctrl+click): after the active one and those already
 * opened from it the same way (`opened`), in order, as in a browser; the
 * active one stays.
 */
export function openBehind({ tabs, active }: TabList, url: string, opened: number): TabList {
  const at = Math.min(active + 1 + opened, tabs.length);
  return { tabs: [...tabs.slice(0, at), { url }, ...tabs.slice(at)], active };
}

/**
 * Closes tab `i` (the last one cannot be closed - null): the same tab stays
 * active, and if it was closed, the left neighbour (the leftmost - the right one).
 */
export function closeAt({ tabs, active }: TabList, i: number): (TabList & { wasActive: boolean }) | null {
  if (i < 0 || i >= tabs.length || tabs.length === 1) return null;
  const rest = tabs.filter((_, j) => j !== i);
  const next = i < active || active >= rest.length ? active - 1 : active;
  return { tabs: rest, active: next, wasActive: i === active };
}

/** The neighbouring tab in a circle: `delta` = ±1. */
export const cycle = (active: number, count: number, delta: number): number => (active + delta + count) % count;

/**
 * Removes the tabs for which `drop` is true (a deleted note): the same tab
 * stays active, and if it was removed, the nearest remaining one to the left
 * (none - to the right). None left - one tab `fallback`.
 */
export function dropTabs({ tabs, active }: TabList, drop: (tab: Tab) => boolean, fallback: string): TabList & { activeDropped: boolean } {
  const keep = tabs.map((t) => !drop(t));
  const rest = tabs.filter((_, i) => keep[i]);
  if (!rest.length) return { tabs: [{ url: fallback }], active: 0, activeDropped: true };
  const activeDropped = !keep[active];
  // The remaining ones left of the active (and itself if it remained): the
  // last of them is the new active one; none on the left - the first on the right.
  const left = keep.slice(0, active + 1).filter(Boolean).length;
  return { tabs: rest, active: clampActive(left - 1, rest.length), activeDropped };
}
