// Часть сайта `static-search.js`: окно поиска по всем заметкам (как палитра
// приложения в режиме поиска). Поиск — `lib/site-search.ts` (правила ядра)
// по индексу сборки (`assets/data/search.js`). Без Svelte: часть маленькая.

import type { SearchDoc, SearchHit } from "../lib/api";
import { search } from "../lib/site-search";
import { pageHref, register } from "./parts";

const LIMIT = 30;

let dialog: HTMLElement | null = null;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

function open(docs: SearchDoc[], base: string): void {
  if (dialog) {
    dialog.querySelector("input")?.focus();
    return;
  }
  const backdrop = el("div", "k-search-backdrop");
  const box = el("div", "k-search");
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-label", "Поиск по заметкам");
  const input = el("input", "k-search-input");
  Object.assign(input, { type: "search", placeholder: "Поиск по тексту заметок", autocomplete: "off", spellcheck: false });
  input.setAttribute("aria-label", "Поиск по тексту заметок");
  const list = el("ul", "k-search-list");
  list.setAttribute("role", "listbox");
  const help = el("div", "k-search-help", "↑↓ — выбрать · Enter — открыть · Ctrl+Enter — в новой вкладке · Esc — закрыть");
  box.append(input, list, help);
  backdrop.append(box);
  document.body.append(backdrop);
  dialog = backdrop;

  let hits: SearchHit[] = [];
  let selected = 0;
  const links = () => [...list.querySelectorAll<HTMLAnchorElement>(".k-search-item")];

  const close = () => {
    backdrop.remove();
    dialog = null;
  };
  const select = (i: number) => {
    const all = links();
    if (!all.length) return;
    selected = (i + all.length) % all.length;
    all.forEach((a, j) => a.parentElement?.classList.toggle("selected", j === selected));
    all[selected]?.parentElement?.scrollIntoView({ block: "nearest" });
  };
  const render = () => {
    const q = input.value.trim();
    hits = q ? search(docs, q, LIMIT) : [];
    list.replaceChildren();
    if (!q) list.append(el("li", "k-search-empty", "Слова через пробел — раздел, где есть все."));
    else if (!hits.length) list.append(el("li", "k-search-empty", "Ничего не нашлось."));
    for (const hit of hits) {
      const li = el("li", "k-search-row");
      li.setAttribute("role", "option");
      const a = el("a", "k-search-item");
      a.href = pageHref(base, hit.id, hit.anchor);
      const head = el("span", "k-search-title", hit.title);
      if (hit.heading) head.append(el("span", "k-search-heading", ` · ${hit.heading}`));
      a.append(head);
      const folder = hit.id.includes("/") ? hit.id.slice(0, hit.id.lastIndexOf("/")) : "";
      if (hit.kind === "book") a.append(el("span", "k-search-badge", "книга"));
      if (folder) a.append(el("span", "k-search-path", folder));
      const snippet = el("span", "k-search-snippet");
      for (const f of hit.snippet) snippet.append(f.hit ? el("mark", "", f.text) : document.createTextNode(f.text));
      a.append(snippet);
      a.addEventListener("click", () => close());
      li.append(a);
      li.addEventListener("mousemove", () => select(links().indexOf(a)));
      list.append(li);
    }
    selected = 0;
    select(0);
  };

  input.addEventListener("input", render);
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      select(selected + (e.key === "ArrowDown" ? 1 : -1));
    } else if (e.key === "Enter") {
      const a = links()[selected];
      if (!a) return;
      e.preventDefault();
      if (e.ctrlKey || e.metaKey) window.open(a.href, "_blank");
      else {
        close();
        location.href = a.href;
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
  });
  backdrop.addEventListener("mousedown", (e) => {
    if (e.target === backdrop) close();
  });
  render();
  input.focus();
}

register("search", { open });
