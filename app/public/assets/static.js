// Статический сайт (notes build): тема и якоря без сервера.
//
// Тема хранится в браузере (localStorage), по умолчанию — как в системе.
// window.K_THEMES — [[имя, тёмная, название], …], подставляется сборкой.

(() => {
  const root = document.documentElement;
  const themes = window.K_THEMES ?? [];
  const names = themes.map(([name]) => name);
  let saved = null;
  try {
    saved = localStorage.getItem("k-theme");
  } catch {}
  const dark = matchMedia("(prefers-color-scheme: dark)").matches;
  const system = (themes.find(([, d]) => d === dark) ?? themes[0] ?? [""])[0];
  root.dataset.theme = names.includes(saved) ? saved : system;

  const scrollToAnchor = () => {
    if (location.hash.length < 2) return;
    const name = decodeURIComponent(location.hash.slice(1));
    const el = document.getElementById(name) ?? document.querySelector(`[data-k-anchor="${CSS.escape(name)}"]`);
    el?.scrollIntoView();
  };

  addEventListener("DOMContentLoaded", () => {
    const button = document.getElementById("k-theme");
    const show = () => (button.textContent = `тема: ${(themes.find(([n]) => n === root.dataset.theme) ?? [])[2] ?? root.dataset.theme}`);
    show();
    button.onclick = () => {
      root.dataset.theme = names[(names.indexOf(root.dataset.theme) + 1) % names.length];
      try {
        localStorage.setItem("k-theme", root.dataset.theme);
      } catch {}
      show();
    };
    scrollToAnchor();
  });
  addEventListener("hashchange", scrollToAnchor);
})();
