// Math in the Russian tradition.
//
// Typst does not know tg/ctg/arctg/sh/ch - without these definitions `tg x`
// is set as the product of variables t*g*x.

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

/// An upright differential: $dd(x)$ -> d x. (Typst also has `dif`.)
#let dd(x) = $dif #x$

/// "Equal by definition".
#let defeq = $:=$

/// A decimal with a comma: $dc("0,5")$.
///
/// IMPORTANT: in Typst math mode a comma is a separator followed by a space,
/// so `$0,5$` prints as "0, 5". The LaTeX trick `0{,}5` does not work either -
/// the braces print literally. The compiler is silent, the mistake shows
/// only on the page; `notes check` catches it (`notes-core::lint`).
#let dc(s) = math.text(s)
