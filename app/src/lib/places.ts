// Reading places and recent notes, pure logic (the state is
// state/places.svelte.ts).

/** A reading place: scroll and the book chapter. */
export interface Place {
  y: number;
  chapter: number | null;
}

/** Remembers the place of a note: fresh ones at the end, those beyond `max` are forgotten. */
export function rememberPlace(places: Record<string, Place>, id: string, place: Place, max: number): Record<string, Place> {
  const next = { ...places };
  delete next[id];
  next[id] = place;
  const ids = Object.keys(next);
  for (const old of ids.slice(0, Math.max(0, ids.length - max))) delete next[old];
  return next;
}

/** Recent notes: the opened one first, no repeats, at most `max`. */
export const pushRecent = (recent: string[], id: string, max: number): string[] => [id, ...recent.filter((r) => r !== id)].slice(0, max);

/** The place from a history entry (for "back"), if it has one. */
export function placeFromHistory(state: unknown): Place | null {
  const s = state as Partial<Place> | null;
  return typeof s?.y === "number" ? { y: s.y, chapter: s.chapter ?? null } : null;
}
