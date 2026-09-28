// Помощники HTML-режима.
//
// Одна и та же заметка собирается в PDF (вёрстка страницами) и в HTML
// (приложение). В HTML-экспорте Typst нет страниц, а `grid`, `stack`,
// `align`, `place`, `v`, `h` игнорируются — вместе с содержимым. Поэтому у
// каждого блока библиотеки две ветки: PDF — вёрстка страницами A4, HTML —
// элементы с классами `k-…`, а вид задаёт CSS (`app/src/baluk-css/`, сборка — `assets/baluk.css`).
//
// Цвета в HTML не вшиваются: их задают CSS-переменные темы. Исключение —
// рисунки (SVG из `html.frame`): заметка компилируется по разу на тему, и
// приложение склеивает варианты каждого `div.k-frame`.

/// HTML ли сейчас собираем. Только внутри `context`.
#let is-web() = target() == "html"

/// Элемент с классом: elem("div", "k-box k-def", body, style: "…").
#let elem(tag, cls, body, ..attrs) = html.elem(tag, attrs: (class: cls) + attrs.named(), body)

/// Рисунок как SVG в обёртке, которую находит склейка тем.
/// kind: "fig" — рисунок библиотеки (на экране чуть крупнее, см. CSS),
///      "layout" — страховка для grid/stack/place: верстается шириной
///      текста страницы PDF, иначе колонки 1fr схлопываются в ноль.
#let frame(body, kind: "fig") = if kind == "fig" {
  elem("div", "k-frame k-fig", html.frame(body))
} else {
  elem("div", "k-frame k-layout", html.frame(block(width: 21cm - 2 * 2.4cm, body)))
}
