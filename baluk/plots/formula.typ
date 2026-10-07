// Parsing and evaluation of interactive figure formulas. The app client
// (`app/src/lib/plot/formula.ts`) repeats this file LINE BY LINE: change one -
// change the other; the cross-check is the fixture
// `tests/vault/Рисунки/Интерактив.typ` (Vitest reads its snapshot).

// ── Formula parsing ───────────────────────────────────────────────────────

#let _functions = (
  sin: 1, cos: 1, tan: 1, asin: 1, acos: 1, atan: 1, exp: 1, ln: 1, log: 1,
  sqrt: 1, abs: 1, floor: 1, ceil: 1, pow: 2,
)
#let _constants = (pi: calc.pi, e: calc.e)

#let _token-re = regex("\d+(?:\.\d+)?(?:[eE][+-]?\d+)?|[\p{L}_][\p{L}\p{N}_]*(?:\.[\p{L}_][\p{L}\p{N}_]*)?|[-+*/(),]|\S")

/// Formula tokens (spaces are skipped).
#let _tokens(src) = src.matches(_token-re).map(m => m.text)

#let _fail(src, txt) = panic("formula \"" + src + "\": " + txt)

/// Parses an expression of level `level` from token `i`: (node, next token).
/// Levels: 0 - sum, 1 - product, 2 - unary sign and operand.
/// Precedence as in Typst: unary minus binds tighter than `*` and `/`.
#let _parse(toks, i, level, names, src) = {
  let ntok = toks.len()
  if level < 2 {
    let ops = if level == 0 { ("+", "-") } else { ("*", "/") }
    let (cur, i) = _parse(toks, i, level + 1, names, src)
    while i < ntok and toks.at(i) in ops {
      let (prm, j) = _parse(toks, i + 1, level + 1, names, src)
      cur = (k: "op", o: toks.at(i), a: cur, b: prm)
      i = j
    }
    return (cur, i)
  }
  if i >= ntok { _fail(src, "the formula ends too early") }
  let cur = toks.at(i)
  if cur == "-" or cur == "+" {
    let (operand, j) = _parse(toks, i + 1, 2, names, src)
    return (if cur == "-" { (k: "neg", a: operand) } else { operand }, j)
  }
  if cur == "(" {
    let (operand, j) = _parse(toks, i + 1, 0, names, src)
    if j >= ntok or toks.at(j) != ")" { _fail(src, "missing `)`") }
    return (operand, j + 1)
  }
  if cur.match(regex("^\d")) != none { return ((k: "n", v: float(cur)), i + 1) }
  if cur.starts-with("calc.") {
    let name = cur.slice(5)
    if name in _constants { return ((k: "n", v: _constants.at(name)), i + 1) }
    if name not in _functions {
      _fail(src, "no function `" + cur + "`; available: calc." + _functions.keys().join(", calc.") + ", calc.pi, calc.e")
    }
    if i + 1 >= ntok or toks.at(i + 1) != "(" { _fail(src, "`" + cur + "` needs parentheses") }
    let fargs = ()
    let j = i + 2
    let (operand, j) = _parse(toks, j, 0, names, src)
    fargs.push(operand)
    while j < ntok and toks.at(j) == "," {
      let (operand, k) = _parse(toks, j + 1, 0, names, src)
      fargs.push(operand)
      j = k
    }
    if j >= ntok or toks.at(j) != ")" { _fail(src, "missing `)` after the arguments of `" + cur + "`") }
    if fargs.len() != _functions.at(name) {
      _fail(src, "`" + cur + "` takes arguments: " + str(_functions.at(name)))
    }
    return ((k: "f", n: name, args: fargs), j + 1)
  }
  if cur in names { return ((k: "v", n: cur), i + 1) }
  if cur == "^" { _fail(src, "write a power as calc.pow(x, 2)") }
  if cur.match(regex("^[\p{L}_]")) != none {
    _fail(src, "unknown name `" + cur + "`; allowed: " + names.join(", ") + ", calc....")
  }
  _fail(src, "unexpected character `" + cur + "`")
}

/// Parses a formula with the variables `names` (x, y, parameters).
#let _formula(src, names) = {
  let toks = _tokens(src)
  if toks.len() == 0 { _fail(src, "empty formula") }
  let (node, i) = _parse(toks, 0, 0, names, src)
  if i < toks.len() and toks.at(i) == "^" { _fail(src, "write a power as calc.pow(x, 2)") }
  if i < toks.len() { _fail(src, "extra `" + toks.at(i) + "`") }
  node
}

// A number is valid: not none, not NaN, not infinity.
#let _valid(v) = v != none and v == v and calc.abs(v) < 1e300

/// The node value with the variables `vars` (a dictionary). Outside the domain
/// (division by zero, the root of a negative...) - none.
#let _eval(nd, vars) = {
  let k = nd.k
  if k == "n" { return nd.v }
  if k == "v" { return vars.at(nd.n) }
  if k == "neg" {
    let a = _eval(nd.a, vars)
    return if a == none { none } else { -a }
  }
  if k == "op" {
    let a = _eval(nd.a, vars)
    if a == none { return none }
    let b = _eval(nd.b, vars)
    if b == none { return none }
    let r = if nd.o == "+" { a + b } else if nd.o == "-" { a - b } else if nd.o == "*" { a * b } else if b == 0 { none } else { a / b }
    return if _valid(r) { r } else { none }
  }
  // a function
  let a = _eval(nd.args.at(0), vars)
  if a == none { return none }
  let fname = nd.n
  let r = if fname == "sin" { calc.sin(a) } else if fname == "cos" { calc.cos(a) } else if fname == "tan" { calc.tan(a) }
  else if fname == "asin" { if calc.abs(a) > 1 { none } else { calc.asin(a).rad() } }
  else if fname == "acos" { if calc.abs(a) > 1 { none } else { calc.acos(a).rad() } }
  else if fname == "atan" { calc.atan(a).rad() }
  else if fname == "exp" { if a > 700 { none } else { calc.exp(a) } }
  else if fname == "ln" { if a <= 0 { none } else { calc.ln(a) } }
  else if fname == "log" { if a <= 0 { none } else { calc.log(a) } }
  else if fname == "sqrt" { if a < 0 { none } else { calc.sqrt(a) } }
  else if fname == "abs" { calc.abs(a) }
  else if fname == "floor" { float(calc.floor(a)) }
  else if fname == "ceil" { float(calc.ceil(a)) }
  else if fname == "pow" {
    let b = _eval(nd.args.at(1), vars)
    if b == none { none }
    else if a == 0 { if b > 0 { 0.0 } else if b == 0 { 1.0 } else { none } }
    else if a < 0 and calc.fract(b) != 0 { none }
    else if b * calc.ln(calc.abs(a)) > 700 { none }
    else { calc.pow(a, b) }
  }
  if _valid(r) { float(r) } else { none }
}
