#import "/_baluk/lib.typ": *
#show: note.with(
  lang: "de",
  // Словаря немецкого в библиотеке нет: свои слова поверх словаря.
  // Чего нет в words (Remark, frame) — из английского словаря.
  words: (definition: "Definition", example: "Beispiel", step: "Schritt", figure: "Abb."),
  title: [Eigene Wörter],
  tags: ("фикстура", "языки"),
)

Слова оформления — из `words:` шаблона. Словаря языка `de` в библиотеке нет,
но свои слова заданы — `notes check` не предупреждает.

#definition(title: [Grenzwert])[Eine Folge $x_n$ konvergiert gegen $a$, wenn $|x_n - a| -> 0$.]

#example[
  #step[Abschätzen] $-1/n <= (sin n)/n <= 1/n$.
]

#remark[Нет в `words:` — слово из английского словаря.]

#fig(canvas(theme => {
  import cetz.draw: *
  axes(x: (-0.2, 2), y: (-0.2, 1.2))
  plot(x => x * x / 4, 0, 2)
}), [Parabel])
