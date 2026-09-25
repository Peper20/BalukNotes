// Книга по главам.
//
// Сервер отдаёт книгу целиком (по сети это ~0,2 МБ), а в страницу
// вставляется одна глава: вёрстка всей книги — сотни миллисекунд на ПК и
// секунды на телефоне. Главы — прямые потомки <article class="k-doc"> от
// одного h2.k-h1 до следующего; всё до первой главы (титул) идёт с первой.

export interface Chapter {
  heading: HTMLElement;
  nodes: Node[];
  /** Номер главы (`data-num`) или пусто. */
  num: string;
  /** Название без номера. */
  title: string;
}

export interface Book {
  /** Разметка страницы: стили и пустой <article>, куда вставляется глава. */
  fragment: DocumentFragment;
  article: Element;
  intro: Node[];
  chapters: Chapter[];
  /** Якорь (id или слаг заголовка) → номер главы. */
  byAnchor: Map<string, number>;
}

const isElement = (n: Node): n is HTMLElement => n.nodeType === 1;

/** HTML страницы → книга по главам; не книга или одна глава — null. */
export function splitBook(html: string, doc: Document = document): Book | null {
  const tpl = doc.createElement("template");
  tpl.innerHTML = html;
  const article = tpl.content.querySelector('article.k-doc[data-doc="книга"]');
  if (!article) return null;
  const kids = [...article.childNodes];
  const starts = kids.flatMap((n, i) => (isElement(n) && n.matches("h2.k-h1") ? [i] : []));
  if (starts.length < 2) return null;
  const chapters = starts.map((start, k): Chapter => {
    const heading = kids[start] as HTMLElement;
    const title = [...heading.childNodes]
      .filter((n) => !(isElement(n) && n.classList.contains("k-num")))
      .map((n) => n.textContent)
      .join("");
    return { heading, nodes: kids.slice(start, starts[k + 1] ?? kids.length), num: heading.dataset.num ?? "", title };
  });
  const byAnchor = new Map<string, number>();
  chapters.forEach((c, k) => {
    for (const n of c.nodes) {
      if (!isElement(n)) continue;
      for (const el of [n, ...n.querySelectorAll<HTMLElement>("[id], [data-k-anchor]")]) {
        if (el.id) byAnchor.set(el.id, k);
        if (el.dataset.kAnchor) byAnchor.set(el.dataset.kAnchor, k);
      }
    }
  });
  const intro = kids.slice(0, starts[0]);
  article.replaceChildren();
  return { fragment: tpl.content, article, intro, chapters, byAnchor };
}

/** Вставить в статью книги главу `k` (у первой — ещё и титул). */
export function showChapter(book: Book, k: number): void {
  const chapter = book.chapters[k];
  if (!chapter) return;
  book.article.replaceChildren(...(k === 0 ? book.intro : []), ...chapter.nodes);
}

/** Элемент с id или якорем — в любой главе, даже не вставленной. */
export function findInBook(book: Book, anchor: string): Element | null {
  const k = book.byAnchor.get(anchor);
  if (k == null) return null;
  for (const n of book.chapters[k]!.nodes) {
    if (!isElement(n)) continue;
    if (n.id === anchor || n.dataset.kAnchor === anchor) return n;
    const el = n.querySelector(`[id="${CSS.escape(anchor)}"], [data-k-anchor="${CSS.escape(anchor)}"]`);
    if (el) return el;
  }
  return null;
}
