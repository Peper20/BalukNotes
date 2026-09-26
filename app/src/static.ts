// Статический сайт (notes build): тема, якоря и живые рисунки без сервера.
// Собирается отдельно (vite.static.config.ts) в assets/static.js и
// подключается в <head>: тема ставится до отрисовки страницы.
//
// Тема хранится в браузере (localStorage), по умолчанию — как в системе.
// window.K_THEMES — [[имя, тёмная, название], …], подставляется сборкой.

import { mountLive } from "./lib/live";

declare global {
  interface Window {
    K_THEMES?: [string, boolean, string][];
  }
}

const root = document.documentElement;
const themes = window.K_THEMES ?? [];
const names = themes.map(([name]) => name);
let saved: string | null = null;
try {
  saved = localStorage.getItem("k-theme");
} catch {
  // хранилище недоступно — тема как в системе
}
const dark = matchMedia("(prefers-color-scheme: dark)").matches;
const system = (themes.find(([, d]) => d === dark) ?? themes[0])?.[0] ?? "";
root.dataset.theme = saved && names.includes(saved) ? saved : system;

function scrollToAnchor() {
  if (location.hash.length < 2) return;
  const name = decodeURIComponent(location.hash.slice(1));
  const el = document.getElementById(name) ?? document.querySelector(`[data-k-anchor="${CSS.escape(name)}"]`);
  el?.scrollIntoView();
}

addEventListener("DOMContentLoaded", () => {
  const button = document.getElementById("k-theme");
  if (button) {
    // Значок — CSS-маской (как в приложении: солнце/луна — какая тема будет по нажатию).
    const next = () => names[(names.indexOf(root.dataset.theme ?? "") + 1) % names.length] ?? "";
    const title = (n: string) => themes.find(([t]) => t === n)?.[2] ?? n;
    const show = () => {
      const n = next();
      button.dataset.next = themes.find(([t]) => t === n)?.[1] ? "dark" : "light";
      button.title = `Тема: ${title(root.dataset.theme ?? "")}. Нажать — «${title(n)}»`;
      button.setAttribute("aria-label", "Тема");
    };
    show();
    button.hidden = false;
    button.onclick = () => {
      root.dataset.theme = next();
      try {
        localStorage.setItem("k-theme", root.dataset.theme ?? "");
      } catch {
        // не запомнили — не страшно
      }
      show();
    };
  }
  const note = document.querySelector("main.k-note");
  // Узел графа — страница сайта рядом: путь от корня сайта (он — у ссылки «Все заметки»).
  const home = document.querySelector<HTMLAnchorElement>(".k-toolbar-home")?.getAttribute("href") ?? "index.html";
  const base = home.replace(/index\.html$/, "");
  const open = (id: string, newTab: boolean) => {
    const url = `${base}${id.split("/").map(encodeURIComponent).join("/")}.html`;
    if (newTab) window.open(url, "_blank");
    else location.href = url;
  };
  if (note) mountLive(note, open);
  const toc = document.querySelector<HTMLDetailsElement>(".k-static-toc");
  if (toc) setupToc(toc);
  scrollToAnchor();
});
addEventListener("hashchange", scrollToAnchor);

/**
 * Оглавление: на широком экране — раскрыто сбоку (как в CSS), на узком —
 * свёрнуто и сворачивается после перехода. Текущий раздел подсвечен.
 */
function setupToc(toc: HTMLDetailsElement) {
  const wide = matchMedia("(min-width: 78rem)");
  const sync = () => (toc.open = wide.matches);
  sync();
  wide.addEventListener("change", sync);
  toc.addEventListener("click", (e) => {
    if ((e.target as Element).closest("a") && !wide.matches) toc.open = false;
  });
  const links = [...toc.querySelectorAll<HTMLAnchorElement>("a[href^='#']")];
  const targets = links.map((a) => document.getElementById(decodeURIComponent(a.hash.slice(1))));
  let frame = 0;
  const mark = () => {
    frame = 0;
    // Текущий — последний раздел, чей заголовок уже выше трети окна; у
    // конца страницы (дальше не прокрутить) — последний видимый.
    const bottom = innerHeight + scrollY >= document.documentElement.scrollHeight - 2;
    const line = bottom ? innerHeight : innerHeight / 3;
    let current = -1;
    targets.forEach((t, i) => {
      if (t && t.getBoundingClientRect().top < line) current = i;
    });
    links.forEach((a, i) => a.classList.toggle("current", i === current));
  };
  addEventListener("scroll", () => (frame ||= requestAnimationFrame(mark)), { passive: true });
  mark();
}
