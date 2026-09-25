// Раскладка графа заметок: простая силовая модель без библиотек. Все узлы
// отталкиваются, рёбра — пружины, слабое притяжение к центру. Узлов —
// десятки–сотни, O(n²) на шаг — миллисекунды; считаем заранее и рисуем уже
// готовый граф. Детерминированно: одинаковый граф — одинаковая картинка.

export type Point = [number, number];

const CUTOFF2 = (4 * 70) ** 2;

export function layout(ids: string[], edges: { from: string; to: string }[]): Map<string, Point> {
  const n = ids.length;
  const index = new Map(ids.map((id, i) => [id, i]));
  const pos: Point[] = ids.map((_, i) => {
    const a = (2 * Math.PI * i) / n;
    return [Math.cos(a) * 100, Math.sin(a) * 100];
  });
  const links = edges
    .map((e) => [index.get(e.from), index.get(e.to)] as const)
    .filter((l): l is readonly [number, number] => l[0] != null && l[1] != null);
  const k = 70; // желаемая длина ребра
  for (let step = 0, t = 30; step < 400; step++, t *= 0.985) {
    const force: Point[] = pos.map(() => [0, 0]);
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        const dx = pos[i]![0] - pos[j]![0];
        const dy = pos[i]![1] - pos[j]![1];
        const d2 = Math.max(dx * dx + dy * dy, 1);
        // Дальше CUTOFF узлы не отталкиваются: иначе несвязанные заметки
        // улетают на края, и связная часть графа сжимается в точку.
        if (d2 > CUTOFF2) continue;
        const f = ((k * k) / d2) * (1 - d2 / CUTOFF2);
        force[i]![0] += dx * f;
        force[i]![1] += dy * f;
        force[j]![0] -= dx * f;
        force[j]![1] -= dy * f;
      }
    }
    for (const [a, b] of links) {
      const dx = pos[a]![0] - pos[b]![0];
      const dy = pos[a]![1] - pos[b]![1];
      const d = Math.max(Math.hypot(dx, dy), 1);
      const f = (d - k) / d / 2;
      force[a]![0] -= dx * f;
      force[a]![1] -= dy * f;
      force[b]![0] += dx * f;
      force[b]![1] += dy * f;
    }
    for (let i = 0; i < n; i++) {
      const p = pos[i]!;
      const fi = force[i]!;
      fi[0] -= p[0] * 0.02;
      fi[1] -= p[1] * 0.02;
      const len = Math.hypot(fi[0], fi[1]);
      if (len > 0) {
        const m = Math.min(len, t) / len;
        p[0] += fi[0] * m;
        p[1] += fi[1] * m;
      }
    }
  }
  return new Map(ids.map((id, i) => [id, pos[i]!]));
}
