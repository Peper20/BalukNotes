// Математика в русской традиции.
//
// Typst не знает tg/ctg/arctg/sh/ch — без этих определений `tg x`
// набирается как произведение переменных t·g·x.

#let tg = math.op("tg")
#let ctg = math.op("ctg")
#let arctg = math.op("arctg")
#let arcctg = math.op("arcctg")
#let sh = math.op("sh")
#let ch = math.op("ch")
#let th = math.op("th")
#let cth = math.op("cth")
#let rot = math.op("rot")
#let grad = math.op("grad")
#let const = math.op("const")

/// Дифференциал прямым шрифтом: $dd(x)$ → d x. (В Typst есть и `dif`.)
#let dd(x) = $dif #x$

/// «Равно по определению».
#let defeq = $:=$

/// Десятичная дробь с запятой: $dc("0,5")$.
///
/// ВАЖНО: запятая в матрежиме Typst — разделитель, после неё ставится
/// пробел, поэтому `$0,5$` печатается как «0, 5». Приём из LaTeX `0{,}5`
/// тоже не работает — скобки выводятся буквально. Компилятор молчит, ошибку
/// видно только на странице; её ловит `lib/lint.py`.
#let dc(s) = math.text(s)
