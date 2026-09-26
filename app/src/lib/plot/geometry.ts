// Геометрия интерактивных рисунков: деления осей, обрезка кривой по полосе,
// проекция и грани поверхности. Повторяет `baluk/plots.typ` — чтобы
// живой рисунок в первый момент совпадал с кадром из Typst.

export type Pt = [number, number];

/** Шаг делений: 1, 2 или 5 × 10^k, около пяти делений на диапазон. */
export function niceStep(d: number): number {
  const s = d / 5;
  const p = Math.pow(10, Math.floor(Math.log10(s)));
  const m = s / p;
  return (m < 1.5 ? 1 : m < 3.5 ? 2 : m < 7.5 ? 5 : 10) * p;
}

/** Число для подписи: знаков после запятой — как у шага, минус типографский. */
export function formatNumber(v: number, step: number, trim = false): string {
  const digits = Math.max(0, -Math.floor(Math.log10(step) + 1e-9));
  let r = Number(v.toFixed(digits));
  if (Math.abs(r) < step / 1e6) r = 0;
  let s = Math.abs(r).toFixed(digits);
  if (trim && s.includes(".")) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return (r < 0 ? "−" : "") + s;
}

/** Деления в [a, b] с шагом `step`. */
export function ticks(a: number, b: number, step: number): number[] {
  const out: number[] = [];
  for (let i = Math.ceil(a / step - 1e-9); i <= Math.floor(b / step + 1e-9); i++) out.push(i * step + 0);
  return out;
}

/** n + 1 точка кривой на [a, b]; вне области определения — null. */
export function sample(f: (x: number) => number | null, a: number, b: number, n: number): [number, number | null][] {
  const out: [number, number | null][] = [];
  for (let i = 0; i <= n; i++) {
    const x = a + ((b - a) * i) / n;
    out.push([x, f(x)]);
  }
  return out;
}

/**
 * Участки кривой внутри полосы y ∈ [y0, y1]. Отрезок, пересекающий полосу,
 * обрезается по её краю; отрезок между точками по разные стороны от полосы
 * — разрыв (асимптота, как у tg x).
 */
export function clipRuns(pts: [number, number | null][], y0: number, y1: number): Pt[][] {
  const all: Pt[][] = [];
  let cur: Pt[] = [];
  const flush = () => {
    if (cur.length > 1) all.push(cur);
    cur = [];
  };
  for (let i = 0; i < pts.length - 1; i++) {
    const [xa, ya] = pts[i]!;
    const [xb, yb] = pts[i + 1]!;
    if (ya === null || yb === null || (ya > y1 && yb < y0) || (ya < y0 && yb > y1)) {
      flush();
      continue;
    }
    let t0 = 0;
    let t1 = 1;
    for (const [edge, above] of [[y0, false], [y1, true]] as const) {
      const outA = above ? ya > edge : ya < edge;
      const outB = above ? yb > edge : yb < edge;
      if (outA && outB) t0 = 2;
      else if (outA) t0 = Math.max(t0, (edge - ya) / (yb - ya));
      else if (outB) t1 = Math.min(t1, (edge - ya) / (yb - ya));
    }
    if (t0 > t1) {
      flush();
      continue;
    }
    const p = (t: number): Pt => [xa + (xb - xa) * t, ya + (yb - ya) * t];
    if (cur.length === 0 || t0 > 0) {
      flush();
      cur = [p(t0)];
    }
    cur.push(p(t1));
    if (t1 < 1) flush();
  }
  flush();
  return all;
}

// ── 3D ──────────────────────────────────────────────────────────────────
// Поверхность нормируется в коробку [-1, 1]² × [-Z, Z] и рисуется
// ортогонально: поворот вокруг вертикали (θ), потом наклон (φ).

export const Z = 0.7;
export type P3 = [number, number, number];

/** Экранные координаты (u вправо, v вверх) и глубина (больше — ближе). */
export function view([x, y, z]: P3, th: number, ph: number): P3 {
  const xr = x * Math.cos(th) - y * Math.sin(th);
  const yr = x * Math.sin(th) + y * Math.cos(th);
  return [xr, z * Math.cos(ph) + yr * Math.sin(ph), -yr * Math.cos(ph) + z * Math.sin(ph)];
}

/** Освещённость 0..1 грани с нормалью n (в осях экрана); обе стороны — одинаково. */
export function lighting([a, b, c]: P3): number {
  const len = Math.hypot(a, b, c);
  if (len === 0) return 0.5;
  return Math.abs(a * -0.38 + b * 0.62 + c * 0.69) / len;
}

export interface Face {
  depth: number;
  /** Вершины на экране (u, v). */
  pts: Pt[];
  light: number;
}

/**
 * Грани поверхности по значениям в узлах сетки (n + 1) × (n + 1), от
 * дальних к ближним (алгоритм художника). Грань с узлом вне [z0, z1] или
 * вне области определения не рисуется.
 */
export function faces(grid: (number | null)[][], z0: number, z1: number, th: number, ph: number): Face[] {
  const n = grid.length - 1;
  const box = (i: number, j: number, v: number): P3 => [-1 + (2 * i) / n, -1 + (2 * j) / n, -Z + (2 * Z * (v - z0)) / (z1 - z0)];
  const out: Face[] = [];
  const at = (i: number, j: number) => grid[i]?.[j] ?? null;
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < n; j++) {
      const [a, b, c, d] = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
      const bad = (v: number | null): v is null => v === null || v < z0 || v > z1;
      if (bad(a) || bad(b) || bad(c) || bad(d)) continue;
      const w = [box(i, j, a), box(i + 1, j, b), box(i + 1, j + 1, c), box(i, j + 1, d)].map((p) => view(p, th, ph));
      const [w0, w1, w2, w3] = w as [P3, P3, P3, P3];
      const d1: P3 = [w2[0] - w0[0], w2[1] - w0[1], w2[2] - w0[2]];
      const d2: P3 = [w3[0] - w1[0], w3[1] - w1[1], w3[2] - w1[2]];
      const nrm: P3 = [d1[1] * d2[2] - d1[2] * d2[1], d1[2] * d2[0] - d1[0] * d2[2], d1[0] * d2[1] - d1[1] * d2[0]];
      out.push({ depth: (w0[2] + w1[2] + w2[2] + w3[2]) / 4, pts: w.map((q) => [q[0], q[1]]), light: lighting(nrm) });
    }
  }
  return out.sort((p, q) => p.depth - q.depth);
}

/** Смесь цветов `#rrggbb[aa]`: k = 0 — a, k = 1 — b. */
export function mixHex(a: string, b: string, k: number): string {
  const pa = parseHex(a);
  const pb = parseHex(b);
  const [r, g, bl, al] = pa.map((v, i) => v + ((pb[i] ?? v) - v) * k) as [number, number, number, number];
  return `rgba(${Math.round(r)}, ${Math.round(g)}, ${Math.round(bl)}, ${(al / 255).toFixed(3)})`;
}

function parseHex(h: string): number[] {
  const s = h.trim().replace(/^#/, "");
  const full = s.length <= 4 ? [...s].map((ch) => ch + ch).join("") : s;
  const v = [0, 2, 4, 6].map((i) => (i < full.length ? parseInt(full.slice(i, i + 2), 16) : 255));
  return v.map((x) => (Number.isNaN(x) ? 0 : x));
}
