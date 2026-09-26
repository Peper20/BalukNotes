// Слова оформления на языке заметки: «Определение», «Рис.», «Глава»…
//
// Язык — параметр `lang:` шаблона (`note`, `book`), по умолчанию "ru"; он
// же ставит `set text(lang:)` (переносы, кавычки). Языка нет в словаре —
// русские слова (`notes check` предупреждает). Свои слова поверх словаря —
// `words: (figure: "Abb.", …)` у шаблона. Новый язык — ещё один словарь с
// теми же ключами.
//
// Сообщения об ошибках библиотеки — по-русски на любом языке заметки.

#let words = (
  ru: (
    definition: "Определение",
    theorem: "Теорема",
    example: "Пример",
    remark: "Замечание",
    pitfall: "Типичная ошибка",
    idea: "Идея",
    algorithm: "Алгоритм",
    step: "Шаг",
    answer: "Ответ",
    proof: "Почему это верно.",
    plan-note: "В этой заметке",
    plan-chapter: "В этой главе",
    summary: "Коротко о главном",
    quiz: "Проверь себя",
    answers: "Ответы",
    mistake: "Ошибка",
    avoid: "Как избежать",
    complexity: "Сложность",
    figure: "Рис.",
    chapter: "Глава",
    contents: "Содержание",
    book-kind: "Конспект",
    frame: "кадр",
    frame-of: "из",
  ),
  en: (
    definition: "Definition",
    theorem: "Theorem",
    example: "Example",
    remark: "Remark",
    pitfall: "Common mistake",
    idea: "Idea",
    algorithm: "Algorithm",
    step: "Step",
    answer: "Answer",
    proof: "Why this holds.",
    plan-note: "In this note",
    plan-chapter: "In this chapter",
    summary: "Key points",
    quiz: "Check yourself",
    answers: "Answers",
    mistake: "Mistake",
    avoid: "How to avoid",
    complexity: "Complexity",
    figure: "Fig.",
    chapter: "Chapter",
    contents: "Contents",
    book-kind: "Notes",
    frame: "frame",
    frame-of: "of",
  ),
)

/// Свои слова документа (`words:` шаблона) — поверх словаря языка.
#let _own-words = state("baluk-words", (:))

/// Проверяет `words:` шаблона: словарь «ключ → строка» с ключами словаря.
#let _check-words(own) = {
  assert(type(own) == dictionary, message: "words — словарь, например (figure: \"Abb.\", chapter: \"Kapitel\")")
  for (key, value) in own {
    assert(key in words.ru, message: "words: нет слова «" + key + "»; есть: " + words.ru.keys().join(", "))
    assert(type(value) == str, message: "words: «" + key + "» — строка, например \"" + words.ru.at(key) + "\"")
  }
  own
}

/// Слово оформления: своё (`words:` шаблона) или из словаря языка текста
/// (`text.lang`); нужен `context`.
#let word(key) = {
  let own = _own-words.get()
  if key in own { own.at(key) } else { words.at(text.lang, default: words.ru).at(key) }
}
