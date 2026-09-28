// Пример навыка /new-note: ЯЗЫК ЗАМЕТКИ. lang: "en" — слова оформления
// по-английски («Definition», «Fig.»); words: — свои слова поверх словаря.
// Для русской заметки lang не пишут (по умолчанию "ru").
#import "/_baluk/lib.typ": *
#show: note.with(
  lang: "en",
  words: (example: "Worked example"),
  title: [Limits],
  tags: ("пример",),                // один тег — всё равно массив: запятая в конце
)

#definition[A sequence $x_n$ converges to $a$ if $|x_n - a|$ eventually stays below any $epsilon > 0$.]

#example[
  #step[Bound] $|1 / n - 0| = 1 / n$.
  #answer[$lim 1 / n = 0$]
]
