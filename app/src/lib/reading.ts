// Показ заметки — чистая логика: какую главу просить, как прокрутить,
// что в оглавлении (состояние — state/reader.svelte.ts).

import type { ChapterSelect, Heading } from "./api";
import type { Place } from "./places";

/** Как прокрутить только что показанную заметку. */
export type ScrollIntent = { mode: "top" } | { mode: "anchor" } | ({ mode: "keep" } & Place);

/** Какую главу просить у сервера (не по главам — сервер отдаст целиком). */
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

/** Прокрутка после загрузки: явная, «как было», к якорю, к месту или к началу. */
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

/** Пункты оглавления: `depth` уровней от верхнего; меньше двух — оглавления нет. */
export function tocItems(headings: Heading[], depth: number): TocItem[] {
  const top = Math.min(...headings.map((h) => h.level));
  const shown = headings.filter((h) => h.level - top < depth).map((h) => ({ ...h, depth: h.level - top }));
  return shown.length >= 2 ? shown : [];
}
