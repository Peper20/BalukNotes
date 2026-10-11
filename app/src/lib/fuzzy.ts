// Fuzzy search for quick open: the query letters must occur in the string in
// order (not necessarily adjacent). Weight for the start of the string and
// of words, for adjacent letters; case-insensitive, "ё" = "е".

export interface Match {
  score: number;
  /** Positions of the matched characters in the string, for highlighting. */
  positions: number[];
}

const fold = (c: string): string => {
  const l = c.toLowerCase();
  return l === "ё" ? "е" : l;
};

const isBoundary = (prev: string | undefined): boolean => prev === undefined || /[\s/_\-.,:(«"]/.test(prev);

/** A match of the query with the string, or null. An empty query matches everything. */
export function fuzzy(query: string, text: string): Match | null {
  const q = [...query.trim()].map(fold).filter((c) => c !== " ");
  if (!q.length) return { score: 0, positions: [] };
  const t = [...text];
  const tf = t.map(fold);

  // First the whole substring (the query as typed, with its spaces, then
  // without them): the best case, we take it.
  const folded = tf.join("");
  const typed = [...query.trim()].map(fold).join("");
  for (const needle of [typed, q.join("")]) {
    const at = folded.indexOf(needle);
    if (at < 0) continue;
    const positions = [...Array(needle.length).keys()].map((i) => at + i);
    return { score: 100 + (at === 0 ? 50 : isBoundary(t[at - 1]) ? 30 : 0) - t.length / 10, positions };
  }

  // Otherwise greedily from left to right, preferring word starts.
  // A word-start jump can run past the letters that follow, so if it fails
  // the plain first occurrences still find the match.
  return scan(q, t, tf, true) ?? scan(q, t, tf, false);
}

function scan(q: string[], t: string[], tf: string[], jump: boolean): Match | null {
  const positions: number[] = [];
  let from = 0;
  let score = 0;
  for (const c of q) {
    let found = -1;
    for (let i = from; i < tf.length; i++) {
      if (tf[i] !== c) continue;
      if (found < 0) found = i;
      if (jump && isBoundary(t[i - 1])) {
        found = i;
        break;
      }
      if (!jump) break;
    }
    if (found < 0) return null;
    const prev = positions.at(-1);
    score += prev !== undefined && found === prev + 1 ? 8 : isBoundary(t[found - 1]) ? 6 : 1;
    positions.push(found);
    from = found + 1;
  }
  return { score: score - t.length / 10, positions };
}

/** The string split into pieces to highlight the matched characters. */
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
