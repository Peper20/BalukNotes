// Поиск без сервера (статический сайт): по индексу, который сборка сайта
// выгружает из ядра (`notes_core::search::documents`). Правила и веса —
// **те же, что у ядра** (`crates/notes-core/src/search.rs`, строка в
// строку): запрос — слова через пробел, раздел подходит, если каждое слово
// есть в тексте, заголовке или названии/пути (подстрокой, без регистра,
// «ё» = «е»); вес: название 20 > заголовок 8 > вхождения в тексте (до 5).
// Правишь одно — правь другое; сверка — `site-search.test.ts` по эталону
// ядра `tests/snapshots/search.json`.

import type { Fragment, SearchDoc, SearchHit, SearchSection } from "./api";

/** Сколько разделов одной заметки показывать. */
const PER_NOTE = 3;
/** Длина фрагмента в символах. */
const SNIPPET = 160;
/** Сколько символов показать до первого совпадения. */
const BEFORE = 40;

type Chars = string[];

/** Сравнение без регистра и «ё»; символ → один символ (позиции совпадают с исходным). */
function fold(c: string): string {
  const l = [...c.toLowerCase()][0] ?? c;
  return l === "ё" ? "е" : l;
}

const chars = (s: string): Chars => [...s];
const folded = (s: string): Chars => chars(s).map(fold);

function find(hay: Chars, needle: Chars, from: number, end = hay.length): number {
  if (!needle.length || needle.length > end) return -1;
  outer: for (let i = from; i <= end - needle.length; i++) {
    for (let j = 0; j < needle.length; j++) if (hay[i + j] !== needle[j]) continue outer;
    return i;
  }
  return -1;
}

function count(hay: Chars, needle: Chars): number {
  let n = 0;
  let i = 0;
  for (let at = find(hay, needle, i); at >= 0; at = find(hay, needle, i)) {
    n++;
    i = at + needle.length;
  }
  return n;
}

/** Поиск по всем заметкам: лучшие разделы каждой, по весу. */
export function search(docs: readonly SearchDoc[], query: string, limit: number): SearchHit[] {
  const hits = scoped(docs, query);
  // Как `sort_by` Rust: устойчиво; названия — по кодам символов.
  hits.sort((a, b) => b.score - a.score || (a.title < b.title ? -1 : a.title > b.title ? 1 : 0));
  return hits.slice(0, limit);
}

function scoped(docs: readonly SearchDoc[], query: string): SearchHit[] {
  const words = query
    .split(/\s+/)
    .filter(Boolean)
    .map((w) => folded(w));
  if (!words.length) return [];
  const hits: SearchHit[] = [];
  for (const doc of docs) {
    const head = folded(`${doc.title} ${doc.id}`);
    let own: SearchHit[] = [];
    for (const section of doc.sections) {
      const hit = matchSection(doc, head, section, words);
      if (hit) own.push(hit);
    }
    // Совпало только название — одна строка на заметку, а не по разделу.
    if (own.every((h) => h.snippet.every((f) => !f.hit))) own = own.slice(0, 1);
    own.sort((a, b) => b.score - a.score);
    hits.push(...own.slice(0, PER_NOTE));
  }
  return hits;
}

function matchSection(doc: SearchDoc, head: Chars, section: SearchSection, words: Chars[]): SearchHit | null {
  const heading = folded(section.heading ?? "");
  const original = chars(section.text);
  const text = original.map(fold);
  let score = 0;
  for (const w of words) {
    const inHead = count(head, w) > 0;
    const inHeading = count(heading, w) > 0;
    const inText = count(text, w);
    if (!inHead && !inHeading && inText === 0) return null;
    score += Number(inHead) * 20 + Number(inHeading) * 8 + Math.min(inText, 5);
  }
  return {
    id: doc.id,
    kind: doc.kind,
    title: doc.title,
    heading: section.heading,
    anchor: section.anchor,
    snippet: snippet(original, text, words),
    score,
  };
}

/** Окно вокруг первого совпадения, по границам слов, с отмеченными словами. */
export function snippet(original: Chars, text: Chars, words: Chars[]): Fragment[] {
  const firsts = words.map((w) => find(text, w, 0)).filter((p) => p >= 0);
  const len = original.length;
  let start = firsts.length ? Math.max(Math.min(...firsts) - BEFORE, 0) : 0;
  if (start > 0) {
    let i = start;
    while (i < len && original[i - 1] !== " ") i++;
    if (i < len) start = i;
  }
  let end = Math.min(start + SNIPPET, len);
  if (end < len) {
    let i = end - 1;
    while (i >= start && original[i] !== " ") i--;
    if (i > start) end = i;
  }
  // Отметки совпадений в окне: [начало, конец) с объединением пересечений.
  const marks: [number, number][] = [];
  for (const w of words) {
    let i = start;
    for (let at = find(text, w, i, end); at >= 0; at = find(text, w, i, end)) {
      marks.push([at, at + w.length]);
      i = at + w.length;
    }
  }
  marks.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  const merged: [number, number][] = [];
  for (const [a, b] of marks) {
    const last = merged.at(-1);
    if (last && a <= last[1]) last[1] = Math.max(last[1], b);
    else merged.push([a, b]);
  }
  const out: Fragment[] = [];
  const push = (text: string, hit: boolean) => {
    if (text) out.push({ text, hit });
  };
  let pos = start;
  let plain = start > 0 ? "…" : "";
  for (const [a, b] of merged) {
    plain += original.slice(pos, a).join("");
    push(plain, false);
    plain = "";
    push(original.slice(a, b).join(""), true);
    pos = b;
  }
  plain += original.slice(pos, end).join("");
  if (end < len) plain += "…";
  push(plain, false);
  return out;
}
