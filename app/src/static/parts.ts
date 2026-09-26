// Части скрипта сайта: `static.js` (тема, оглавление — у каждой страницы)
// маленький, остальное — отдельными файлами `assets/static-<часть>.js`,
// которые он подгружает, только когда они нужны странице. Часть — IIFE,
// она кладёт свой вход в `window.balukParts[имя]` (`register`). Данные
// сборки (индекс поиска, граф) — так же, файлами `assets/data/<имя>.js`
// (`notes-site::data`). Обычный `<script>`, а не `import()`/`fetch`: сайт
// открывается и с диска (file://).

import type { GraphLayout, SearchDoc } from "../lib/api";
import type { OpenNote } from "../lib/live/block";

/** Что отдаёт каждая часть. */
export interface Parts {
  /** Живые блоки заметки (`lib/live`): рисунки, кадры, граф. */
  live: { mountLive(root: Element, open: OpenNote): () => void };
  /** Поиск по сайту (`search.ts`). */
  search: { open(docs: SearchDoc[], base: string): void };
  /** Страница графа (`graph.ts`). */
  graph: { mount(target: HTMLElement, layout: GraphLayout, open: OpenNote): void };
}

/** Данные сборки сайта. */
export interface Data {
  search: SearchDoc[];
  graph: GraphLayout;
}

declare global {
  interface Window {
    balukParts?: Partial<Parts>;
    balukData?: Partial<Data>;
  }
}

/** Вызывает часть при загрузке. */
export function register<K extends keyof Parts>(name: K, part: Parts[K]): void {
  (window.balukParts ??= {})[name] = part;
}

// Каталог assets/ — рядом со static.js; запоминается, пока скрипт
// выполняется синхронно (`currentScript` потом — null).
const own = document.currentScript instanceof HTMLScriptElement ? document.currentScript.src : "";
const assets = own.replace(/[^/]*$/, "");

const loading = new Map<string, Promise<unknown>>();

/** Подключить скрипт (один раз) и взять то, что он положил в `window`. */
function inject<T>(src: string, take: () => T | undefined): Promise<T> {
  let p = loading.get(src) as Promise<T> | undefined;
  if (!p) {
    p = new Promise<T>((resolve, reject) => {
      const s = document.createElement("script");
      s.src = src;
      s.onload = () => {
        const got = take();
        if (got !== undefined) resolve(got);
        else reject(new Error(`${src}: нет данных после загрузки`));
      };
      s.onerror = () => reject(new Error(`не загрузился ${src}`));
      document.head.append(s);
    });
    loading.set(src, p);
  }
  return p;
}

/** Загрузить часть; промис — её вход. */
export function load<K extends keyof Parts>(name: K): Promise<Parts[K]> {
  return inject(`${assets}static-${name}.js`, () => window.balukParts?.[name] as Parts[K] | undefined);
}

/** Загрузить данные сборки. */
export function loadData<K extends keyof Data>(name: K): Promise<Data[K]> {
  return inject(`${assets}data/${name}.js`, () => window.balukData?.[name] as Data[K] | undefined);
}

/** Корень сайта (по ссылке «Все заметки»: у каждой страницы своя глубина). */
export function siteBase(): string {
  const home = document.querySelector<HTMLAnchorElement>(".k-toolbar-home")?.getAttribute("href") ?? "index.html";
  return home.replace(/index\.html$/, "");
}

/** Адрес страницы заметки от корня сайта `base` (как `notes-site`: путь + `.html`). */
export function pageHref(base: string, id: string, anchor?: string | null): string {
  const path = `${base}${id.split("/").map(encodeURIComponent).join("/")}.html`;
  return anchor ? `${path}#${encodeURIComponent(anchor)}` : path;
}
