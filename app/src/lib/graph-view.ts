// The note graph on screen: colors, search, zoom and pan. What to show (the
// filter) and where (the layout) is computed by the core (`notes-core::vault_graph`).

/** Group colors are theme variables: the graph recolors with the theme. */
const GROUP_COLORS = ["--k-accent", "--k-box-example", "--k-second", "--k-box-idea", "--k-box-algo", "--k-box-pitfall", "--k-box-thm"];
export const groupColor = (group: string, groups: string[]): string =>
  `var(${GROUP_COLORS[Math.max(groups.indexOf(group), 0) % GROUP_COLORS.length]})`;

/** Node label: font size and gap from the circle (in layout units), as in the core (`vault_graph.rs`). */
export const LABEL_SIZE = 11;
export const LABEL_GAP = 2;

/** Whether a node matches the search: by path and title, case-insensitive. */
export function matches(query: string, id: string, title?: string | null): boolean {
  const q = query.trim().toLocaleLowerCase("ru");
  return q !== "" && (id.toLocaleLowerCase("ru").includes(q) || (title ?? "").toLocaleLowerCase("ru").includes(q));
}

// -- Zoom and pan --------------------------------------------------------

/** Screen = world · k + (x, y). */
export interface View {
  x: number;
  y: number;
  k: number;
}

export const MIN_ZOOM = 0.15;
export const MAX_ZOOM = 6;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** Zooms by `factor` so that the screen point (px, py) stays in place. */
export function zoomAt(v: View, factor: number, px: number, py: number): View {
  const k = clamp(v.k * factor, MIN_ZOOM, MAX_ZOOM);
  const f = k / v.k;
  return { x: px - (px - v.x) * f, y: py - (py - v.y) * f, k };
}

/** Fit no larger: a graph of a couple of notes does not stretch over the whole screen. */
export const FIT_ZOOM = 1.6;

/** Fits the world rectangle [x0, y0, x1, y1] into a w×h screen with margins `pad`; no zoom above `FIT_ZOOM`. */
export function fitView([x0, y0, x1, y1]: [number, number, number, number], w: number, h: number, pad = 24): View {
  const k = clamp(Math.min((w - 2 * pad) / Math.max(x1 - x0, 1), (h - 2 * pad) / Math.max(y1 - y0, 1)), MIN_ZOOM, FIT_ZOOM);
  return { k, x: (w - (x1 - x0) * k) / 2 - x0 * k, y: (h - (y1 - y0) * k) / 2 - y0 * k };
}
