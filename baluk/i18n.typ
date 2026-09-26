// Слова оформления на языке заметки: «Определение», «Рис.», «Глава»…
//
// Язык — параметр `lang:` шаблона (`note`, `book`), по умолчанию "ru"; он
// же ставит `set text(lang:)` (переносы, кавычки). Языка нет в словаре —
// русские слова. Новый язык — ещё один словарь с теми же ключами.
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

/// Слово оформления на языке текста (`text.lang`); нужен `context`.
#let word(key) = words.at(text.lang, default: words.ru).at(key)
