#import "/_baluk/lib.typ": *

= Код
#lead[Реализация: построение за $O(n)$ и запрос за $O(1)$ (идея — в разделе @sec-prefix).]

== Листинг

// listing: код в ```язык … ```; highlight — строки, callouts — кружки у строк, complexity — сложность.
#listing(
  caption: [построение префиксов],
  highlight: (4,),
  callouts: ("4": 1),
  complexity: [$O(n)$],
  ```python
  def prefix(a):
      p = [0] * (len(a) + 1)
      for i, x in enumerate(a):
          p[i + 1] = p[i] + x
      return p
  ```,
)

Строка #callout(1) — каждый префикс на один элемент длиннее предыдущего.

== Код из файла

// code-from-file: путь ОТ КОРНЯ ХРАНИЛИЩА, начинается с «/»; регион — строки
// между «// region: имя» и «// endregion: имя» в самом файле (язык — по расширению).
#code-from-file("/examples/book/code/prefix.cpp", region: "query", caption: [запрос], complexity: [$O(1)$])

== Граф хранилища

// Граф заметок прямо в тексте: заметки с тегом «пример» и ссылки между ними.
// Ещё фильтры: around: "Папка/Заметка", depth: 2 — соседи; folders: ("Папка",).
#vault-graph(tag: "пример", width: 8cm)
