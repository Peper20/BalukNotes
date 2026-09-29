// Граф заметок на экране: цвета, поиск, масштаб и сдвиг. Что показать
// (фильтр) и где (раскладка) — считает ядро (`notes-core::vault_graph`).

/** Цвета групп — переменные темы: граф перекрашивается вместе с темой. */
const GROUP_COLORS = ["--k-accent", "--k-box-example", "--k-second", "--k-box-idea", "--k-box-algo", "--k-box-pitfall", "--k-box-thm"];
export const groupColor = (group: string, groups: string[]): string =>
  `var(${GROUP_COLORS[Math.max(groups.indexOf(group), 0) % GROUP_COLORS.length]})`;

/** Подпись узла: кегль и отступ от кружка (в единицах раскладки) — как в ядре (`vault_graph.rs`). */
export const LABEL_SIZE = 11;
export const LABEL_GAP = 2;

/** Совпадает ли узел с поиском: по пути и названию, без учёта регистра. */
export function matches(query: string, id: string, title?: string | null): boolean {
  const q = query.trim().toLocaleLowerCase("ru");
  return q !== "" && (id.toLocaleLowerCase("ru").includes(q) || (title ?? "").toLocaleLowerCase("ru").includes(q));
}

// ── Масштаб и сдвиг ─────────────────────────────────────────────────────

/** Экран = мир · k + (x, y). */
export interface View {
  x: number;
  y: number;
  k: number;
}

export const MIN_ZOOM = 0.15;
export const MAX_ZOOM = 6;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** Увеличить в `factor` раз так, чтобы точка экрана (px, py) осталась на месте. */
export function zoomAt(v: View, factor: number, px: number, py: number): View {
  const k = clamp(v.k * factor, MIN_ZOOM, MAX_ZOOM);
  const f = k / v.k;
  return { x: px - (px - v.x) * f, y: py - (py - v.y) * f, k };
}

/** Крупнее не вписывать: граф из пары заметок не растягивается во весь экран. */
export const FIT_ZOOM = 1.6;

/** Вписать прямоугольник мира [x0, y0, x1, y1] в экран w×h с полями `pad`; крупнее `FIT_ZOOM` — не увеличивать. */
export function fitView([x0, y0, x1, y1]: [number, number, number, number], w: number, h: number, pad = 24): View {
  const k = clamp(Math.min((w - 2 * pad) / Math.max(x1 - x0, 1), (h - 2 * pad) / Math.max(y1 - y0, 1)), MIN_ZOOM, FIT_ZOOM);
  return { k, x: (w - (x1 - x0) * k) / 2 - x0 * k, y: (h - (y1 - y0) * k) / 2 - y0 * k };
}
