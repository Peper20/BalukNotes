// Тема до загрузки клиента: без неё первый кадр — пустой фон браузера
// (белый в тёмной теме). Что показать — запомнил клиент (`settings.apply`,
// `themeMemo` в src/lib/appearance.ts): выбранная тема или своя для светлой
// и тёмной системы.
// Своя у каждого хранилища (`k-theme@<имя>`, адрес /v/<имя>/…), иначе —
// последняя показанная.
try {
  var m = /^\/v\/([^/]+)/.exec(location.pathname);
  var own = m && localStorage.getItem("k-theme@" + decodeURIComponent(m[1]));
  var t = JSON.parse(own || localStorage.getItem("k-theme") || "null");
  var name = t && (t.fixed || (matchMedia("(prefers-color-scheme: dark)").matches ? t.dark : t.light));
  if (typeof name === "string") document.documentElement.dataset.theme = name;
} catch (e) {
  // нет localStorage — тема придёт с настройками
}
