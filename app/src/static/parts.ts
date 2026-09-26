// Части скрипта сайта: `static.js` (тема, оглавление — у каждой страницы)
// маленький, остальное — отдельными файлами `assets/static-<часть>.js`,
// которые он подгружает, только когда они нужны странице. Часть — IIFE,
// она кладёт свой вход в `window.balukParts[имя]` (`register`). Обычный
// `<script>`, а не `import()`/`fetch`: сайт открывается и с диска (file://).

import type { OpenNote } from "../lib/live/block";

/** Что отдаёт каждая часть. */
export interface Parts {
  /** Живые блоки заметки (`lib/live`): рисунки, кадры, граф. */
  live: { mountLive(root: Element, open: OpenNote): () => void };
}

declare global {
  interface Window {
    balukParts?: Partial<Parts>;
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

/** Загрузить часть (один раз); промис — её вход. */
export function load<K extends keyof Parts>(name: K): Promise<Parts[K]> {
  let p = loading.get(name) as Promise<Parts[K]> | undefined;
  if (!p) {
    p = new Promise<Parts[K]>((resolve, reject) => {
      const s = document.createElement("script");
      s.src = `${assets}static-${name}.js`;
      s.onload = () => {
        const part = window.balukParts?.[name];
        if (part) resolve(part as Parts[K]);
        else reject(new Error(`часть сайта ${name} не зарегистрировалась`));
      };
      s.onerror = () => reject(new Error(`не загрузилась часть сайта ${s.src}`));
      document.head.append(s);
    });
    loading.set(name, p);
  }
  return p;
}
