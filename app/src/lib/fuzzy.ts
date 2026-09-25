// Нечёткий поиск для быстрого перехода: буквы запроса должны встретиться
// в строке по порядку (не обязательно подряд). Вес — за начало строки и
// слов, за буквы подряд; без учёта регистра, «ё» = «е».

export interface Match {
  score: number;
  /** Позиции совпавших символов в строке — для подсветки. */
  positions: number[];
}

const fold = (c: string): string => {
  const l = c.toLowerCase();
  return l === "ё" ? "е" : l;
};

const isBoundary = (prev: string | undefined): boolean => prev === undefined || /[\s/_\-.,:(«"]/.test(prev);

/** Совпадение запроса со строкой или null. Пустой запрос совпадает со всем. */
export function fuzzy(query: string, text: string): Match | null {
  const q = [...query.trim()].map(fold).filter((c) => c !== " ");
  if (!q.length) return { score: 0, positions: [] };
  const t = [...text];
  const tf = t.map(fold);

  // Сначала — подстрока целиком: лучший случай, его и берём.
  const joined = q.join("");
  const at = tf.join("").indexOf(joined);
  if (at >= 0 && t.length === tf.join("").length) {
    const positions = [...Array(q.length).keys()].map((i) => at + i);
    return { score: 100 + (at === 0 ? 50 : isBoundary(t[at - 1]) ? 30 : 0) - t.length / 10, positions };
  }

  // Иначе — жадно слева направо, предпочитая начала слов.
  const positions: number[] = [];
  let from = 0;
  let score = 0;
  for (const c of q) {
    let found = -1;
    for (let i = from; i < tf.length; i++) {
      if (tf[i] !== c) continue;
      if (found < 0) found = i;
      if (isBoundary(t[i - 1])) {
        found = i;
        break;
      }
    }
    if (found < 0) return null;
    const prev = positions.at(-1);
    score += prev !== undefined && found === prev + 1 ? 8 : isBoundary(t[found - 1]) ? 6 : 1;
    positions.push(found);
    from = found + 1;
  }
  return { score: score - t.length / 10, positions };
}

/** Строка, разбитая на куски для подсветки совпавших символов. */
export function highlight(text: string, positions: number[]): { text: string; hit: boolean }[] {
  const chars = [...text];
  const set = new Set(positions);
  const out: { text: string; hit: boolean }[] = [];
  chars.forEach((c, i) => {
    const hit = set.has(i);
    const last = out.at(-1);
    if (last && last.hit === hit) last.text += c;
    else out.push({ text: c, hit });
  });
  return out;
}
