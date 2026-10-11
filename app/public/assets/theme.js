// The theme before the client loads: without it the first frame is the
// empty browser background (white in a dark theme). What to show was
// remembered by the client (`settings.apply`, `themeMemo` in
// src/lib/appearance.ts): the chosen theme or its own for a light and a dark
// system. Per vault (`k-theme@<name>`, the address /v/<name>/...), otherwise
// the last shown one.
try {
  var m = /^\/v\/([^/]+)/.exec(location.pathname);
  var own = m && localStorage.getItem("k-theme@" + decodeURIComponent(m[1]));
  var t = JSON.parse(own || localStorage.getItem("k-theme") || "null");
  // Not `name`: in a classic script that is `window.name`, which turns null into the string "null".
  var chosen = t && (t.fixed || (matchMedia("(prefers-color-scheme: dark)").matches ? t.dark : t.light));
  if (typeof chosen === "string") document.documentElement.dataset.theme = chosen;
} catch (e) {
  // no localStorage: the theme comes with the settings
}
