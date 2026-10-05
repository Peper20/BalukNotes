// Showing a note, pure logic: which chapter to request, how to scroll, what
// is in the outline (the state is state/reader.svelte.ts).

import type { ChapterSelect, Heading } from "./api";
import type { Place } from "./places";

/** How to scroll a note just shown. */
export type ScrollIntent = { mode: "top" } | { mode: "anchor" } | ({ mode: "keep" } & Place);

/** Which chapter to request from the server (not by chapters - the server gives it whole). */
export function chapterSelect(
  byChapters: boolean,
  { chapter, keepScroll, current, anchor, place }: { chapter?: number; keepScroll: boolean; current: number | null; anchor: string | null; place: Place | null },
): ChapterSelect {
  if (!byChapters) return {};
  if (chapter != null) return { chapter };
  if (keepScroll && current != null) return { chapter: current };
  if (anchor) return { anchor };
  return { chapter: place?.chapter ?? 0 };
}

/** Scroll after loading: explicit, "as it was", to the anchor, to the place or to the start. */
export function scrollIntent(
  explicit: ScrollIntent | undefined,
  { keepScroll, y, current, anchor, place }: { keepScroll: boolean; y: number; current: number | null; anchor: string | null; place: Place | null },
): ScrollIntent {
  if (explicit) return explicit;
  if (keepScroll) return { mode: "keep", y, chapter: current };
  if (anchor) return { mode: "anchor" };
  return place ? { mode: "keep", ...place } : { mode: "top" };
}

export type TocItem = Heading & { depth: number };

/** Outline items: `depth` levels from the top; fewer than two - no outline. */
export function tocItems(headings: Heading[], depth: number): TocItem[] {
  const top = Math.min(...headings.map((h) => h.level));
  const shown = headings.filter((h) => h.level - top < depth).map((h) => ({ ...h, depth: h.level - top }));
  return shown.length >= 2 ? shown : [];
}
