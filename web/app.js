// Клиент заметок: дерево, показ заметки, обновление по запросу, настройки.
//
// Сервер отдаёт готовый HTML заметки (/api/notes/…); клиент только вставляет
// его и применяет настройки вида атрибутами на <html> (правила — в
// konspekt.css). Обновление — по кнопке и раз в N секунд сверкой версии
// (/api/version/… — дёшево, сервер ничего не компилирует, если файлы не
// менялись). Форма настроек строится по схеме с сервера.
//
// Временный клиент вехи M1 без сборки и фреймворка; интерфейс M2 его заменит.

const $ = (sel) => document.querySelector(sel);
const root = document.documentElement;

const state = {
  schema: null,
  settings: {},
  themes: [], // [{name, dark}]
  notes: [], // [{id, kind, name, folder}]
  current: null, // id выбранной заметки (показана или ещё собирается)
  version: null, // версия показанной заметки
  timer: null,
  pending: null, // AbortController загрузки, которая сейчас идёт
};

// ── API ──────────────────────────────────────────────────────────────────

/** Путь заметки в URL: сегменты кодируются, «/» остаётся. */
const encodeId = (id) => id.split("/").map(encodeURIComponent).join("/");

/** Отменить загрузку заметки, если она идёт: пользователь ушёл дальше. */
function cancelPending() {
  state.pending?.abort();
  state.pending = null;
}

async function api(path, options) {
  const res = await fetch(path, options);
  const body = await res.json().catch(() => ({}));
  if (!res.ok) {
    const err = new Error(body.error || `${res.status} ${res.statusText}`);
    err.status = res.status;
    throw err;
  }
  return body;
}

// ── Настройки ────────────────────────────────────────────────────────────

const darkQuery = matchMedia("(prefers-color-scheme: dark)");

function resolveTheme(value) {
  if (value !== "auto" && state.themes.some((t) => t.name === value)) return value;
  const wantDark = darkQuery.matches;
  const match = state.themes.find((t) => t.dark === wantDark) ?? state.themes[0];
  return match?.name ?? "";
}

function applySettings() {
  const v = state.settings;
  root.dataset.theme = resolveTheme(v["appearance.theme"]);
  root.style.setProperty("--k-size", `${v["appearance.font_size"]}px`);
  root.style.setProperty("--k-measure", `${v["appearance.measure"]}em`);
  root.dataset.numbering = v["headings.numbering"];
  root.dataset.chapters = v["headings.chapters"];
  for (const part of ["title", "kind", "description", "byline", "tags"]) {
    root.dataset[`header${part[0].toUpperCase()}${part.slice(1)}`] = String(v[`header.${part}`]);
  }
  $("#theme").title = `Тема: ${v["appearance.theme"] === "auto" ? `как в системе (${root.dataset.theme})` : root.dataset.theme}`;
  schedule();
}

async function saveSettings(patch) {
  const error = $("#settings-error");
  try {
    state.settings = await api("/api/settings", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(patch),
    });
    error.hidden = true;
    applySettings();
    // Настройки отрисовки (figures.*) меняют версию страницы на сервере.
    if (Object.keys(patch).some((k) => k.startsWith("figures."))) check();
  } catch (e) {
    error.textContent = e.message;
    error.hidden = false;
  }
}

function settingInput(def, value) {
  let input;
  if (def.type === "bool") {
    input = Object.assign(document.createElement("input"), { type: "checkbox", checked: value });
    input.onchange = () => saveSettings({ [def.key]: input.checked });
  } else if (def.type === "number") {
    input = Object.assign(document.createElement("input"), { type: "number", min: def.min, max: def.max, step: def.step, value });
    input.onchange = () => input.reportValidity() && saveSettings({ [def.key]: Number(input.value) });
  } else {
    input = document.createElement("select");
    for (const o of def.options) input.add(new Option(o.label, o.value, false, o.value === value));
    input.onchange = () => saveSettings({ [def.key]: input.value });
  }
  input.id = `setting-${def.key}`;
  return input;
}

function openSettings() {
  const body = $("#settings-body");
  body.replaceChildren();
  for (const group of state.schema.groups) {
    const fs = document.createElement("fieldset");
    fs.append(Object.assign(document.createElement("legend"), { textContent: group.label }));
    for (const def of state.schema.settings.filter((s) => s.key.startsWith(`${group.key}.`))) {
      const row = document.createElement("div");
      row.className = "setting";
      const label = Object.assign(document.createElement("label"), { textContent: def.label, htmlFor: `setting-${def.key}` });
      row.append(label, settingInput(def, state.settings[def.key]));
      if (def.help) row.append(Object.assign(document.createElement("div"), { className: "help", textContent: def.help }));
      fs.append(row);
    }
    body.append(fs);
  }
  $("#settings-error").hidden = true;
  $("#settings").showModal();
}

function cycleTheme() {
  const options = ["auto", ...state.themes.map((t) => t.name)];
  const i = options.indexOf(state.settings["appearance.theme"]);
  saveSettings({ "appearance.theme": options[(i + 1) % options.length] });
}

// ── Дерево заметок ───────────────────────────────────────────────────────

function renderTree() {
  const tree = { folders: new Map(), notes: [] };
  for (const note of state.notes) {
    let node = tree;
    for (const part of note.folder ? note.folder.split("/") : []) {
      if (!node.folders.has(part)) node.folders.set(part, { folders: new Map(), notes: [] });
      node = node.folders.get(part);
    }
    node.notes.push(note);
  }
  const build = (node) => {
    const items = document.createElement("div");
    for (const [name, child] of [...node.folders].sort(([a], [b]) => a.localeCompare(b, "ru"))) {
      const details = Object.assign(document.createElement("details"), { open: true });
      details.append(Object.assign(document.createElement("summary"), { textContent: name }));
      const inner = build(child);
      inner.className = "items";
      details.append(inner);
      items.append(details);
    }
    for (const note of node.notes) {
      const a = Object.assign(document.createElement("a"), { href: `/n/${encodeId(note.id)}`, textContent: note.name });
      a.dataset.id = note.id;
      if (note.kind === "book") a.classList.add("book");
      items.append(a);
    }
    return items;
  };
  $("#tree").replaceChildren(build(tree));
  markActive();
}

function markActive() {
  for (const a of document.querySelectorAll("#tree a")) a.classList.toggle("active", a.dataset.id === state.current);
}

// ── Заметка ──────────────────────────────────────────────────────────────

function setStatus(text, busy = false) {
  const s = $("#status");
  s.textContent = text;
  s.classList.toggle("busy", busy);
}

const time = () => new Date().toLocaleTimeString("ru-RU");

function renderProblems(page) {
  const box = $("#problems");
  box.replaceChildren();
  const item = (d) => {
    const li = document.createElement("li");
    if (d.file) {
      const where = [d.file, d.line, d.column].filter((x) => x != null).join(":");
      li.append(Object.assign(document.createElement("span"), { className: "where", textContent: `${where} ` }));
    }
    li.append(Object.assign(document.createElement("code"), { textContent: d.message }));
    for (const h of d.hints ?? []) li.append(Object.assign(document.createElement("div"), { className: "where", textContent: `подсказка: ${h}` }));
    return li;
  };
  if (page.errors.length) {
    const div = Object.assign(document.createElement("div"), { className: "errors" });
    const title = page.rendered ? "Не собралось — показана прошлая версия" : "Не собралось";
    div.append(Object.assign(document.createElement("h3"), { textContent: title }));
    const ul = document.createElement("ul");
    ul.append(...page.errors.map(item));
    div.append(ul);
    box.append(div);
  }
  if (page.warnings.length) {
    const details = document.createElement("details");
    details.append(Object.assign(document.createElement("summary"), { textContent: `Предупреждения: ${page.warnings.length}` }));
    const ul = document.createElement("ul");
    ul.append(...page.warnings.map(item));
    details.append(ul);
    box.append(details);
  }
  box.hidden = !box.childElementCount;
}

function scrollToAnchor(hash) {
  if (!hash || hash.length < 2) return false;
  const name = decodeURIComponent(hash.slice(1));
  const el = document.getElementById(name) ?? $("#note").querySelector(`[data-k-anchor="${CSS.escape(name)}"]`);
  el?.scrollIntoView();
  return Boolean(el);
}

/**
 * Шрифты догружаются уже после вставки заметки (font-display: swap), и
 * текст выше якоря перестраивается — якорь уезжает. Пару секунд после
 * перехода возвращаемся к нему после каждой догрузки шрифтов.
 */
function holdAnchor(id) {
  const hash = location.hash;
  const again = () => state.current === id && location.hash === hash && scrollToAnchor(hash);
  document.fonts.addEventListener("loadingdone", again);
  requestAnimationFrame(() => document.fonts.ready.then(again));
  setTimeout(() => document.fonts.removeEventListener("loadingdone", again), 2000);
}

/**
 * Показать заметку. Большая заметка собирается секунды; если за это время
 * выбрали другую, прежний запрос отменяется, а его ответ (если успел)
 * отбрасывается — иначе клиент «перепрыгнул» бы назад.
 */
async function loadNote(id, { keepScroll = false } = {}) {
  cancelPending();
  const ctrl = new AbortController();
  state.pending = ctrl;
  if (state.current !== id) state.version = null;
  state.current = id;
  markActive();
  setStatus("собираю…", true);
  try {
    const page = await api(`/api/notes/${encodeId(id)}`, { signal: ctrl.signal });
    if (state.pending !== ctrl) return;
    state.version = page.version;
    const y = scrollY;
    root.dataset.kind = page.kind;
    const r = page.rendered;
    $("#note").innerHTML = r ? r.styles + r.body : "";
    renderProblems(page);
    const note = state.notes.find((n) => n.id === id);
    document.title = `${r?.title ?? note?.name ?? id} — Заметки`;
    $("#crumbs").replaceChildren(
      ...(note?.folder ? [document.createTextNode(`${note.folder} / `)] : []),
      Object.assign(document.createElement("b"), { textContent: note?.name ?? id }),
    );
    markActive();
    if (keepScroll) scrollTo(0, y);
    else if (scrollToAnchor(location.hash)) holdAnchor(id);
    else scrollTo(0, 0);
    setStatus(`собрано ${time()}`);
  } catch (e) {
    if (state.pending !== ctrl) return; // отменена или устарела
    state.version = null;
    $("#note").replaceChildren(Object.assign(document.createElement("p"), {
      className: "welcome",
      textContent: e.status === 404 ? `Заметки «${id}» нет.` : `Не удалось загрузить: ${e.message}`,
    }));
    $("#problems").hidden = true;
    setStatus("");
  } finally {
    if (state.pending === ctrl) state.pending = null;
  }
}

function showIndex() {
  cancelPending();
  state.current = null;
  state.version = null;
  delete root.dataset.kind;
  document.title = "Заметки";
  $("#crumbs").textContent = "";
  $("#problems").hidden = true;
  markActive();
  const box = document.createElement("div");
  box.className = "welcome";
  box.append(Object.assign(document.createElement("p"), {
    textContent: state.notes.length ? "Выберите заметку:" : "Хранилище пусто: положите .typ-файлы в data/vault/.",
  }));
  const ul = document.createElement("ul");
  for (const n of state.notes) {
    const li = document.createElement("li");
    li.append(Object.assign(document.createElement("a"), { href: `/n/${encodeId(n.id)}`, textContent: n.id }));
    ul.append(li);
  }
  box.append(ul);
  $("#note").replaceChildren(box);
}

// ── Обновление ───────────────────────────────────────────────────────────

/** Изменились ли файлы заметки — и если да, перезагрузить её. */
async function check({ force = false } = {}) {
  const id = state.current;
  if (!id || state.pending) return;
  if (force) return loadNote(id, { keepScroll: true });
  try {
    const { version } = await api(`/api/version/${encodeId(id)}`);
    // Пока ждали ответ, могли перейти на другую заметку.
    if (state.current === id && !state.pending && version !== state.version) await loadNote(id, { keepScroll: true });
  } catch {
    // сервер недоступен — попробуем в следующий раз
  }
}

function schedule() {
  clearInterval(state.timer);
  const seconds = Number(state.settings["refresh.interval"]);
  if (seconds > 0) state.timer = setInterval(() => document.hidden || check(), seconds * 1000);
}

// ── Навигация ────────────────────────────────────────────────────────────

function route() {
  const path = decodeURIComponent(location.pathname);
  if (path.startsWith("/n/")) loadNote(path.slice(3));
  else showIndex();
}

function onClick(e) {
  const a = e.target.closest("a[href]");
  if (!a || a.target || e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
  const url = new URL(a.href, location.href);
  if (url.origin !== location.origin) return;
  if (url.pathname === location.pathname) {
    // Якорь в той же заметке: прокрутка сама, запоминаем в истории.
    if (url.hash) {
      e.preventDefault();
      history.pushState(null, "", url);
      scrollToAnchor(url.hash);
    }
    return;
  }
  if (url.pathname === "/" || url.pathname.startsWith("/n/")) {
    e.preventDefault();
    history.pushState(null, "", url);
    closeSidebarOnMobile();
    route();
  }
}

const mobile = matchMedia("(max-width: 800px)");

function toggleSidebar() {
  $("#app").classList.toggle(mobile.matches ? "sidebar-open" : "sidebar-hidden");
}

function closeSidebarOnMobile() {
  if (mobile.matches) $("#app").classList.remove("sidebar-open");
}

// ── Запуск ───────────────────────────────────────────────────────────────

async function init() {
  const [settings, themes, notes] = await Promise.all([api("/api/settings"), api("/api/themes"), api("/api/notes")]);
  state.schema = settings.schema;
  state.settings = settings.values;
  state.themes = themes;
  state.notes = notes;
  applySettings();
  renderTree();
  route();

  addEventListener("popstate", route);
  document.addEventListener("click", onClick);
  darkQuery.addEventListener("change", applySettings);
  addEventListener("focus", () => state.settings["refresh.on_focus"] && check());
  $("#refresh").onclick = () => check({ force: true });
  $("#theme").onclick = cycleTheme;
  $("#open-settings").onclick = openSettings;
  $("#toggle-sidebar").onclick = toggleSidebar;
  $("#backdrop").onclick = closeSidebarOnMobile;
  addEventListener("keydown", (e) => {
    if (e.key === "r" && !e.ctrlKey && !e.metaKey && !e.altKey && !e.target.closest("input, select, textarea")) check({ force: true });
  });
}

init().catch((e) => {
  document.body.textContent = `Не удалось запустить клиент: ${e.message}`;
});
