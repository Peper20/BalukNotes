// Layout words in the note's language: "Definition", "Fig.", "Chapter"...
//
// The language is the template's `lang:` parameter (`note`, `book`), "ru" by
// default; it also sets `set text(lang:)` (hyphenation, quotes). A language
// missing from the dictionaries gets English words (`notes check` warns).
// Own words over the dictionary - the template's `words: (figure: "Abb.",
// ...)`; what `words:` lacks also comes from English. The dictionaries are
// complete and share keys (test `--test library`); a new language is one
// more dictionary with the same keys.
//
// Library error messages are in English whatever the note's language.

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

/// The document's own words (the template's `words:`) over the language dictionary.
#let _own-words = state("baluk-words", (:))

/// Checks the template's `words:`: a dictionary "key -> string" with dictionary keys.
#let _check-words(own) = {
  assert(type(own) == dictionary, message: "words is a dictionary, e.g. (figure: \"Abb.\", chapter: \"Kapitel\")")
  for (key, value) in own {
    assert(key in words.en, message: "words: no word \"" + key + "\"; known: " + words.en.keys().join(", "))
    assert(type(value) == str, message: "words: \"" + key + "\" is a string, e.g. \"" + words.en.at(key) + "\"")
  }
  own
}

/// A layout word: own (the template's `words:`) or from the dictionary of the
/// text language (`text.lang`), English if the language has none; needs `context`.
#let word(key) = {
  let own = _own-words.get()
  if key in own { own.at(key) } else { words.at(text.lang, default: words.en).at(key) }
}
