// Места чтения и недавние заметки — чистая логика (состояние —
// state/places.svelte.ts).

/** Место чтения: прокрутка и глава книги. */
export interface Place {
  y: number;
  chapter: number | null;
}

/** Запомнить место заметки: свежие — в конце, старше `max` — забыть. */
export function rememberPlace(places: Record<string, Place>, id: string, place: Place, max: number): Record<string, Place> {
  const next = { ...places };
  delete next[id];
  next[id] = place;
  const ids = Object.keys(next);
  for (const old of ids.slice(0, Math.max(0, ids.length - max))) delete next[old];
  return next;
}

/** Недавние: открытая — первой, без повторов, не больше `max`. */
export const pushRecent = (recent: string[], id: string, max: number): string[] => [id, ...recent.filter((r) => r !== id)].slice(0, max);

/** Место из записи истории (для «назад»), если оно там есть. */
export function placeFromHistory(state: unknown): Place | null {
  const s = state as Partial<Place> | null;
  return typeof s?.y === "number" ? { y: s.y, chapter: s.chapter ?? null } : null;
}
