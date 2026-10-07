---
paths:
  - "tests/**"
---

# tests/vault - the fixture vault

End-to-end tests (`crates/*/tests/`), reference snapshots (`tests/snapshots/`),
the visual check and the test server (`.claude/rules/tools.md`) run on it.

**Every file in `tests/vault/` checks something.** A new rendering case - a new
file, a row in the catalog below, snapshots (`UPDATE_SNAPSHOTS=1 cargo test -p
notes-core --test it snapshots::`) and, if the number of notes or problems
changes, the "Total" line.

## Intended problems

`notes --vault tests/vault check` finds exactly these (exit code 1):

| Note | Problem |
|---|---|
| `Особые случаи/Ошибка компиляции` | an unknown function; a broken link `Нет/Из несобравшейся` (links of a failed note come from the source) |
| `Особые случаи/Предупреждение` | a Typst warning (`#h(1fr)` in HTML) and two from the app's checks: `lang: "uk"` without a dictionary and without `words:`; `;` after `#see(...)` disappears |
| `Особые случаи/Ссылки` | a broken link to a note and a broken anchor |
| `Сеть/UFW` | a broken link `Сеть/Nginx` |
| `Глубоко/а/б/_folder.toml` | an unknown key `titel` (the folder shows its own name) |

The total is the last output line; `tools/check.sh` and the test
`check_finds_exactly_the_planted_problems` compare it:

Total: `notes: 22, errors: 2, warnings: 4, broken links: 4`

## Catalog

| File | What it checks |
|---|---|
| `Сеть/SSH.typ` | an ordinary note: tags, heading anchors, links with an anchor, a figure merged by colors |
| `Сеть/UFW.typ` | a link with an anchor and own text; a broken link |
| `демо/компоненты.typ` | all blocks, listings, code colors, tight brackets in formulas |
| `демо/визуализация.typ` | all kinds of figures: charts, plots, 3D, arrays, graphs, trees; figure precision |
| `ассессмент/01-тесты.typ` | a large chapter with `book` (outside a book): code from the files in `code/` |
| `Книга/` | a book: title page, `k-h1` chapters, own chapter tags (`chapter.with`: the first with the label `гл-основы`, the third is `=` without tags), the same "Итоги" in chapters -> `Итоги`, `Итоги-2`, `Итоги-3`; the label `<особый>` -> `id`; `code-from-file` with a region; an image from a file (`img/схема.svg`); links inside the book |
| `Особые случаи/Без шаблона.typ` | plain Typst without the library: headings in the contents, formulas |
| `Особые случаи/Ошибка компиляции.typ` | an error with file and line; the "Не собралось" stub; links are checked from the source |
| `Особые случаи/Предупреждение.typ` | a Typst warning (fractional `h`) and `lint` (a language without a dictionary, `;`, `$0,5$`); absolute `#h`/`#v` -> `span.k-h`/`div.k-v` (negative too), `quad` in a formula is left alone |
| `Особые случаи/Запасные шрифты.typ` | glyphs missing from the theme main fonts: text from New Computer Modern (whole file), code from DejaVu Sans Mono (in parts); bold and italic |
| `Особые случаи/English/` | a book with `lang: "en"`: layout words from `i18n.typ`, `lang` on `<article>` |
| `Особые случаи/Свои слова.typ` | `lang: "de"` with own `words:`; what `words:` lacks comes from the English dictionary; no warning |
| `Особые случаи/Ссылки.typ` | all kinds of `#see`: by text, by label, into a book chapter, to names with `+ # %`, to a section with symbols, deep, a computed path; broken ones; repeated headings; `\;` after a call |
| `Имена/C++ и C#.typ` | spaces, `+`, `#` in a name; headings "C", "C++", "C#" are different anchors (`%23` in the address); "a=b" and "a b" differ |
| `Имена/50% готово.typ` | `%` in a name: double decoding of the address |
| `Имена/_черновик.typ` | an internal file (`_`) is not a note |
| `Имена/_folder.toml` | the folder title in the tree, graph and header instead of the name |
| `Имена/странное.typ` | a title of characters forbidden in file names; a link without text shows the target title |
| `Глубоко/а/б/в/г/Дно.typ` | five nesting levels; `а/б` has an error in `_folder.toml` |
| `Рисунки/Темы и градиент.typ` | the figure shape depends on the theme (the fallback: SVG per theme); a gradient |
| `Рисунки/Интерактив.typ` | `k-plot`: a plot with sliders, several curves, gaps (tg, 1/x, a root), a surface; a table of values from Typst is the reference for the client parser (Vitest) |
| `Рисунки/Граф хранилища.typ` | `vault-graph` with a filter (neighbors; a folder without unwritten notes): data `/_vault/graph/...`, `div.k-graph[data-k-graph]`; rebuilt only when the graph changed |
| `Рисунки/Кадры.typ` | `k-frames`: a range of integers (Riemann sums), a fractional step with own caption and `loop`, explicit values (sorting steps); without JS and in PDF - the default frame, steps in PDF in a row (`pdf: (1, 2, 3, 4)`) |
| `Формулы и теги.typ` | a note in the root; tags with a space; tall brackets, matrices, systems, `dc`, Russian operators; a formula in a heading (the contents get the heading HTML) |
| `_служебное/`, `.скрытое/` | internal folders are not shown in the tree |
