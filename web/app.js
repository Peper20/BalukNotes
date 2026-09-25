// Клиент заметок: дерево, показ заметки, обновление по запросу, настройки.
//
// Сервер отдаёт готовый HTML заметки (/api/notes/…); клиент только вставляет
// его и применяет настройки вида атрибутами на <html> (правила — в
// konspekt.css). Обновление — по кнопке и раз в N секунд сверкой версии
// (/api/version/… — дёшево, сервер ничего не компилирует, если файлы не
// менялись). Форма настроек строится по схеме с сервера.
//
// Временный клиент вехи M1 без сборки и фреймворка; интерфейс M2 его заменит.

import { renderGraph } from "./graph.js";

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
  headings: [], // заголовки показанной заметки [{level, id, anchor, text}]
  tocLinks: [], // [[заголовок в заметке, ссылка в оглавлении]] — для подсветки
  book: null, // книга по главам: см. splitBook()
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
  root.dataset.toc = String(v["panels.toc"]);
  root.dataset.backlinks = String(v["panels.backlinks"]);
  renderToc();
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
    // Настройки отрисовки (figures.*) меняют версию страницы на сервере,
    // вид книги (books.*) — раскладку уже полученной страницы.
    if (Object.keys(patch).some((k) => k.startsWith("figures."))) check();
    else if (state.current && Object.keys(patch).some((k) => k.startsWith("books."))) loadNote(state.current);
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
  const chapter = state.book?.byAnchor.get(name);
  if (chapter != null && chapter !== state.book.current) showChapter(chapter);
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
  if (state.current !== id) {
    // Другая заметка: сразу убираем прежнюю — пока новая собирается, на
    // экране не должно быть чужого текста под новым заголовком.
    state.version = null;
    clearNoteUi();
    setHeader(id);
    showLoading(id, ctrl);
    scrollTo(0, 0);
  }
  state.current = id;
  markActive();
  setStatus("собираю…", true);
  try {
    const page = await api(`/api/notes/${encodeId(id)}`, { signal: ctrl.signal });
    if (state.pending !== ctrl) return;
    state.version = page.version;
    const y = scrollY;
    root.dataset.kind = page.kind;
    $("#pdf").hidden = false;
    const r = page.rendered;
    const chapter = keepScroll ? state.book?.current : null;
    state.book = r && state.settings["books.pages"] === "chapters" ? splitBook(r.styles + r.body) : null;
    if (state.book) showChapter(chapter ?? chapterOfHash() ?? 0);
    else {
      $("#note").innerHTML = r ? r.styles + r.body : "";
      $("#chapter-nav").hidden = true;
    }
    renderProblems(page);
    state.headings = r?.headings ?? [];
    renderToc();
    loadBacklinks(id);
    setHeader(id, r?.title);
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

/** Убрать всё, что относится к показанной заметке. */
function clearNoteUi() {
  state.book = null;
  state.headings = [];
  renderToc();
  for (const sel of ["#chapter-nav", "#pdf", "#backlinks", "#problems"]) $(sel).hidden = true;
}

/** Путь в верхней строке и заголовок вкладки. */
function setHeader(id, title) {
  const note = state.notes.find((n) => n.id === id);
  document.title = `${title ?? note?.name ?? id} — Заметки`;
  $("#crumbs").replaceChildren(
    ...(note?.folder ? [document.createTextNode(`${note.folder} / `)] : []),
    Object.assign(document.createElement("b"), { textContent: note?.name ?? id }),
  );
}

/**
 * Заглушка «собирается» на месте заметки. Появляется с задержкой (CSS),
 * поэтому заметка из кэша сменяет её раньше, чем она станет видна; у долгой
 * сборки — счётчик секунд и объяснение.
 */
function showLoading(id, ctrl) {
  const note = state.notes.find((n) => n.id === id);
  const box = Object.assign(document.createElement("div"), { className: "loading" });
  box.setAttribute("role", "status");
  const seconds = Object.assign(document.createElement("span"), { textContent: "" });
  const hint = Object.assign(document.createElement("p"), {
    className: "loading-hint",
    textContent: note?.kind === "book"
      ? "Книга собирается целиком в двух темах — первый раз это несколько секунд. Дальше она открывается сразу, даже после перезапуска."
      : "Первая сборка заметки; дальше она открывается сразу.",
  });
  hint.hidden = true;
  box.append(
    Object.assign(document.createElement("div"), { className: "loading-name", textContent: note?.name ?? id }),
    Object.assign(document.createElement("div"), { className: "loading-bar" }),
    Object.assign(document.createElement("p"), { className: "loading-text", textContent: "Собирается… " }),
    hint,
  );
  box.querySelector(".loading-text").append(seconds);
  $("#note").replaceChildren(box);
  const started = Date.now();
  const timer = setInterval(() => {
    if (state.pending !== ctrl) return clearInterval(timer);
    const s = Math.round((Date.now() - started) / 1000);
    seconds.textContent = `${s} с`;
    hint.hidden = s < 2;
  }, 500);
}

function showIndex() {
  cancelPending();
  state.current = null;
  state.version = null;
  clearNoteUi();
  delete root.dataset.kind;
  document.title = "Заметки";
  $("#crumbs").textContent = "";
  markActive();
  const home = Object.assign(document.createElement("div"), { className: "home" });
  const h = (tag, text, cls) => Object.assign(document.createElement(tag), { textContent: text, className: cls ?? "" });
  home.append(h("h1", "Заметки"));
  if (!state.notes.length) {
    home.append(h("p", "Хранилище пусто: положите .typ-файлы в data/vault/."));
    $("#note").replaceChildren(home);
    return;
  }
  const lead = h("p", "", "home-lead");
  const books = state.notes.filter((n) => n.kind === "book").length;
  lead.append(`${state.notes.length - books} заметок и ${books} книг${books === 1 ? "а" : books > 1 && books < 5 ? "и" : ""}.`);
  if (state.notes.some((n) => n.id === "Начало")) {
    lead.append(" Начните с ", Object.assign(document.createElement("a"), { href: "/n/Начало", textContent: "экскурсии по возможностям" }), ".");
  }
  const graph = Object.assign(document.createElement("section"), { className: "graph" });
  const hint = h("p", "Наведите на узел — подсветятся его связи; нажмите — откроется заметка. Крупные узлы — книги, пустой — заметка, на которую ссылаются, но её ещё нет.", "graph-hint");
  home.append(lead, graph, hint, h("h2", "Все заметки"));
  const byFolder = Map.groupBy(state.notes, (n) => n.folder || "—");
  const list = Object.assign(document.createElement("div"), { className: "home-list" });
  for (const [folder, notes] of byFolder) {
    const group = document.createElement("div");
    group.append(h("h3", folder));
    const ul = document.createElement("ul");
    for (const n of notes) {
      const li = document.createElement("li");
      li.append(Object.assign(document.createElement("a"), { href: `/n/${encodeId(n.id)}`, textContent: n.name }));
      if (n.kind === "book") li.append(h("span", " книга", "home-kind"));
      ul.append(li);
    }
    group.append(ul);
    list.append(group);
  }
  home.append(list);
  $("#note").replaceChildren(home);
  api("/api/graph")
    .then((g) => {
      if (state.current !== null || !graph.isConnected) return;
      renderGraph(graph, g, { onOpen: (id) => navigate(`/n/${encodeId(id)}`) });
    })
    .catch(() => graph.remove());
}

/** Перейти внутри клиента (как по ссылке). */
function navigate(url) {
  history.pushState(null, "", url);
  closeSidebarOnMobile();
  route();
}

/**
 * Список заметок с сервера. Заметки создаёт Claude Code в любой момент —
 * дерево обновляется при каждой проверке, без перезагрузки страницы.
 */
async function refreshNotes() {
  let notes;
  try {
    notes = await api("/api/notes");
  } catch {
    return;
  }
  if (JSON.stringify(notes) === JSON.stringify(state.notes)) return;
  state.notes = notes;
  renderTree();
  if (state.current === null && !state.pending) showIndex();
}

// ── Книга по главам ──────────────────────────────────────────────────────
//
// Сервер отдаёт книгу целиком (по сети это ~0,2 МБ), а в страницу
// вставляется одна глава: вёрстка всей книги — сотни миллисекунд на ПК и
// секунды на телефоне. Главы — прямые потомки <article class="k-doc"> от
// одного h2.k-h1 до следующего; всё до первой главы (титул) идёт с первой.

/** HTML книги → {article, intro, chapters: [{heading, nodes}], byAnchor, current}; не книга — null. */
function splitBook(html) {
  const tpl = document.createElement("template");
  tpl.innerHTML = html;
  const article = tpl.content.querySelector('article.k-doc[data-doc="книга"]');
  if (!article) return null;
  const kids = [...article.childNodes];
  const starts = kids.flatMap((n, i) => (n.nodeType === 1 && n.matches("h2.k-h1") ? [i] : []));
  if (starts.length < 2) return null;
  const chapters = starts.map((start, k) => ({ heading: kids[start], nodes: kids.slice(start, starts[k + 1] ?? kids.length) }));
  // Якорь (id или слаг заголовка) → номер главы: для ссылок и оглавления.
  const byAnchor = new Map();
  chapters.forEach((c, k) => {
    for (const n of c.nodes) {
      if (n.nodeType !== 1) continue;
      for (const el of [n, ...n.querySelectorAll("[id], [data-k-anchor]")]) {
        if (el.id) byAnchor.set(el.id, k);
        if (el.dataset.kAnchor) byAnchor.set(el.dataset.kAnchor, k);
      }
    }
  });
  const intro = kids.slice(0, starts[0]);
  article.replaceChildren();
  $("#note").replaceChildren(tpl.content);
  return { article, intro, chapters, byAnchor, current: null };
}

function chapterOfHash() {
  return location.hash.length > 1 ? state.book.byAnchor.get(decodeURIComponent(location.hash.slice(1))) : undefined;
}

function showChapter(k) {
  const book = state.book;
  book.current = k;
  book.article.replaceChildren(...(k === 0 ? book.intro : []), ...book.chapters[k].nodes);
  const nav = $("#chapter-nav");
  const link = (i, cls, label) => {
    const h = book.chapters[i].heading;
    const a = Object.assign(document.createElement("a"), { className: cls, href: `#${encodeURIComponent(h.id)}` });
    const num = h.dataset.num ? `${h.dataset.num}. ` : "";
    const title = [...h.childNodes].filter((n) => !n.classList?.contains("k-num")).map((n) => n.textContent).join("");
    a.append(Object.assign(document.createElement("small"), { textContent: label }), `${num}${title}`);
    return a;
  };
  nav.replaceChildren(
    ...(k > 0 ? [link(k - 1, "prev", "← предыдущая глава")] : []),
    ...(k + 1 < book.chapters.length ? [link(k + 1, "next", "следующая глава →")] : []),
  );
  nav.hidden = false;
  markCurrentHeading();
}

// ── Оглавление и обратные ссылки ─────────────────────────────────────────

/** Оглавление по заголовкам заметки: `panels.toc_depth` уровней от верхнего. */
function renderToc() {
  const toc = $("#toc");
  const depth = Number(state.settings["panels.toc_depth"] ?? 2);
  const top = Math.min(...state.headings.map((h) => h.level));
  const shown = state.headings.filter((h) => h.level - top < depth);
  $("#toggle-toc").hidden = shown.length < 2;
  if (shown.length < 2) {
    toc.replaceChildren();
    toc.classList.remove("open");
    state.tocLinks = [];
    return;
  }
  const title = Object.assign(document.createElement("div"), { className: "toc-title", textContent: "Содержание" });
  state.tocLinks = [];
  const links = shown.map((h) => {
    const a = Object.assign(document.createElement("a"), { href: `#${encodeURIComponent(h.id)}`, textContent: h.text });
    a.dataset.depth = h.level - top;
    const target = document.getElementById(h.id) ?? findInBook(h.id);
    if (target) state.tocLinks.push([target, a]);
    return a;
  });
  toc.replaceChildren(title, ...links);
  layoutToc();
  markCurrentHeading();
}

/** Элемент с id в невидимой главе книги. */
function findInBook(id) {
  const k = state.book?.byAnchor.get(id);
  if (k == null) return null;
  for (const n of state.book.chapters[k].nodes) {
    if (n.nodeType !== 1) continue;
    if (n.id === id) return n;
    const el = n.querySelector(`[id="${CSS.escape(id)}"]`);
    if (el) return el;
  }
  return null;
}

/** Хватает ли места справа от колонки заметки для оглавления. */
function layoutToc() {
  const note = $("#note").getBoundingClientRect();
  const room = innerWidth - note.right;
  const fits = room >= 200;
  $("#app").classList.toggle("toc-room", fits);
  if (fits) root.style.setProperty("--toc-w", `${Math.min(room - 32, 300)}px`);
  if (fits && state.settings["panels.toc"]) $("#toc").classList.remove("open");
}

/** Подсветить в оглавлении раздел, который сейчас читают. */
function markCurrentHeading() {
  let current = null;
  // Докрутили до конца — последние разделы до верха окна не доедут.
  if (innerHeight + scrollY >= document.documentElement.scrollHeight - 2) {
    current = state.tocLinks.findLast(([heading]) => heading.isConnected)?.[1];
  }
  else {
    for (const [heading, link] of state.tocLinks) {
      if (!heading.isConnected) continue; // другая глава книги
      if (heading.getBoundingClientRect().top > 90) break;
      current = link;
    }
  }
  current ??= state.tocLinks.find(([heading]) => heading.isConnected)?.[1];
  for (const [, link] of state.tocLinks) link.classList.toggle("current", link === current);
  if (current && $("#app").classList.contains("toc-room")) current.scrollIntoView({ block: "nearest" });
}

let scrollFrame = 0;
function onScroll() {
  if (scrollFrame) return;
  scrollFrame = requestAnimationFrame(() => {
    scrollFrame = 0;
    markCurrentHeading();
  });
}

/** § — на широком экране прячет/показывает боковое оглавление (настройка), на узком — всплывающее. */
function toggleToc() {
  const toc = $("#toc");
  if (!state.tocLinks.length) return;
  if ($("#app").classList.contains("toc-room") && !toc.classList.contains("open")) {
    saveSettings({ "panels.toc": !state.settings["panels.toc"] });
  } else {
    toc.classList.toggle("open");
    if (toc.classList.contains("open")) toc.querySelector("a.current")?.scrollIntoView({ block: "nearest" });
  }
}

/** Кто ссылается на заметку — из индекса ссылок (без компиляции). */
async function loadBacklinks(id) {
  const box = $("#backlinks");
  let links;
  try {
    links = await api(`/api/links/${encodeId(id)}`);
  } catch {
    return;
  }
  if (state.current !== id) return;
  box.hidden = !links.backlinks.length;
  if (box.hidden) return;
  const ul = document.createElement("ul");
  for (const b of links.backlinks) {
    const li = document.createElement("li");
    const note = state.notes.find((n) => n.id === b.from);
    li.append(Object.assign(document.createElement("a"), { href: `/n/${encodeId(b.from)}`, textContent: note?.name ?? b.from }));
    if (note?.folder) li.append(Object.assign(document.createElement("span"), { className: "anchor", textContent: ` · ${note.folder}` }));
    if (b.anchor) li.append(Object.assign(document.createElement("span"), { className: "anchor", textContent: ` → «${b.anchor}»` }));
    ul.append(li);
  }
  box.replaceChildren(Object.assign(document.createElement("h2"), { textContent: `Ссылаются сюда · ${links.backlinks.length}` }), ul);
}

// ── Обновление ───────────────────────────────────────────────────────────

/** Изменились ли файлы заметки — и если да, перезагрузить её. */
async function check({ force = false } = {}) {
  refreshNotes();
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
  const id = path.startsWith("/n/") ? path.slice(3) : null;
  // «Назад» по якорям той же заметки — без перезагрузки (у книги — смена главы).
  if (id && id === state.current && !state.pending) {
    if (scrollToAnchor(location.hash)) return;
    if (state.book) showChapter(0);
    scrollTo(0, 0);
    return;
  }
  if (id) loadNote(id);
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
  $("#toggle-toc").onclick = toggleToc;
  // PDF собирается секунды — открываем в новой вкладке, браузер покажет его сам.
  $("#pdf").onclick = () =>
    state.current && open(`/api/pdf/${encodeId(state.current)}?theme=${encodeURIComponent(root.dataset.theme)}`, "_blank");
  $("#toc").addEventListener("click", (e) => e.target.closest("a") && !$("#app").classList.contains("toc-room") && $("#toc").classList.remove("open"));
  addEventListener("scroll", onScroll, { passive: true });
  new ResizeObserver(layoutToc).observe($(".main"));
  $("#backdrop").onclick = closeSidebarOnMobile;
  addEventListener("keydown", (e) => {
    if (e.key === "Escape") $("#toc").classList.remove("open");
    if (e.key === "r" && !e.ctrlKey && !e.metaKey && !e.altKey && !e.target.closest("input, select, textarea")) check({ force: true });
  });
}

init().catch((e) => {
  document.body.textContent = `Не удалось запустить клиент: ${e.message}`;
});
