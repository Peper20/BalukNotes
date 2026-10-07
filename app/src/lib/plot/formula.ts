// A figure formula: a string of Typst code from a subset. Parsing and
// evaluation repeat `baluk/plots/formula.typ` (`_parse`, `_eval`) line by
// line: change one, change the other (the error texts too). The check
// against Typst is formula.test.ts (it reads the snapshot of the fixture
// `Рисунки/Интерактив`).

export type Node =
  | { k: "n"; v: number }
  | { k: "v"; n: string }
  | { k: "neg"; a: Node }
  | { k: "op"; o: "+" | "-" | "*" | "/"; a: Node; b: Node }
  | { k: "f"; n: string; args: Node[] };

const FUNCTIONS: Partial<Record<string, number>> = {
  sin: 1, cos: 1, tan: 1, asin: 1, acos: 1, atan: 1, exp: 1, ln: 1, log: 1,
  sqrt: 1, abs: 1, floor: 1, ceil: 1, pow: 2,
};
const CONSTANTS: Partial<Record<string, number>> = { pi: Math.PI, e: Math.E };

const TOKEN = /\d+(?:\.\d+)?(?:[eE][+-]?\d+)?|[\p{L}_][\p{L}\p{N}_]*(?:\.[\p{L}_][\p{L}\p{N}_]*)?|[-+*/(),]|\S/gu;

export class FormulaError extends Error {}

/** Formula tokens (spaces are skipped). */
export function tokens(s: string): string[] {
  return s.match(TOKEN) ?? [];
}

/** Parses a formula with the variables `names` (x, y, parameters). */
export function parse(s: string, names: string[]): Node {
  const toks = tokens(s);
  // A token by index; past the end, an empty string.
  const t = (i: number) => toks[i] ?? "";
  const end = toks.length;
  const fail = (msg: string): never => {
    throw new FormulaError(`formula "${s}": ${msg}`);
  };
  if (end === 0) fail("empty formula");

  // Levels: 0 - sum, 1 - product, 2 - unary sign and operand. Precedence as
  // in Typst: unary minus binds tighter than `*` and `/`.
  function level(i: number, lv: number): [Node, number] {
    if (lv < 2) {
      const ops = lv === 0 ? ["+", "-"] : ["*", "/"];
      let [l, j] = level(i, lv + 1);
      while (j < end && ops.includes(t(j))) {
        const [r, k] = level(j + 1, lv + 1);
        l = { k: "op", o: t(j) as "+" | "-" | "*" | "/", a: l, b: r };
        j = k;
      }
      return [l, j];
    }
    if (i >= end) fail("the formula ends too early");
    const tok = t(i);
    if (tok === "-" || tok === "+") {
      const [a, j] = level(i + 1, 2);
      return [tok === "-" ? { k: "neg", a } : a, j];
    }
    if (tok === "(") {
      const [a, j] = level(i + 1, 0);
      if (j >= end || t(j) !== ")") fail("missing `)`");
      return [a, j + 1];
    }
    if (/^\d/.test(tok)) return [{ k: "n", v: Number(tok) }, i + 1];
    if (tok.startsWith("calc.")) {
      const name = tok.slice(5);
      const constant = CONSTANTS[name];
      if (constant !== undefined) return [{ k: "n", v: constant }, i + 1];
      const arity = FUNCTIONS[name];
      if (arity === undefined) return fail(`no function \`${tok}\``);
      if (t(i + 1) !== "(") fail(`\`${tok}\` needs parentheses`);
      const args: Node[] = [];
      let [a, j] = level(i + 2, 0);
      args.push(a);
      while (j < end && t(j) === ",") {
        [a, j] = level(j + 1, 0);
        args.push(a);
      }
      if (j >= end || t(j) !== ")") fail(`missing \`)\` after the arguments of \`${tok}\``);
      if (args.length !== arity) fail(`\`${tok}\` takes arguments: ${arity}`);
      return [{ k: "f", n: name, args }, j + 1];
    }
    if (names.includes(tok)) return [{ k: "v", n: tok }, i + 1];
    if (tok === "^") fail("write a power as calc.pow(x, 2)");
    if (/^[\p{L}_]/u.test(tok)) fail(`unknown name \`${tok}\``);
    return fail(`unexpected character \`${tok}\``);
  }

  const [node, i] = level(0, 0);
  if (t(i) === "^") fail("write a power as calc.pow(x, 2)");
  if (i < end) fail(`extra \`${t(i)}\``);
  return node;
}

/** A number is valid: not NaN and not infinity (as `_valid` in Typst). */
const ok = (v: number | null): v is number => v !== null && Math.abs(v) < 1e300;

const frac = (v: number) => v - Math.trunc(v);

/**
 * The value of a node at the variables `vars`. Outside the domain (division
 * by zero, a root of a negative...) - null: a gap on the plot.
 */
export function evaluate(u: Node, vars: Record<string, number>): number | null {
  switch (u.k) {
    case "n":
      return u.v;
    case "v":
      return vars[u.n] ?? null;
    case "neg": {
      const a = evaluate(u.a, vars);
      return a === null ? null : -a;
    }
    case "op": {
      const a = evaluate(u.a, vars);
      if (a === null) return null;
      const b = evaluate(u.b, vars);
      if (b === null) return null;
      const r = u.o === "+" ? a + b : u.o === "-" ? a - b : u.o === "*" ? a * b : b === 0 ? null : a / b;
      return ok(r) ? r : null;
    }
    case "f": {
      const [first, second] = u.args;
      const a = first ? evaluate(first, vars) : null;
      if (a === null) return null;
      let r: number | null;
      switch (u.n) {
        case "sin": r = Math.sin(a); break;
        case "cos": r = Math.cos(a); break;
        case "tan": r = Math.tan(a); break;
        case "asin": r = Math.abs(a) > 1 ? null : Math.asin(a); break;
        case "acos": r = Math.abs(a) > 1 ? null : Math.acos(a); break;
        case "atan": r = Math.atan(a); break;
        case "exp": r = a > 700 ? null : Math.exp(a); break;
        case "ln": r = a <= 0 ? null : Math.log(a); break;
        case "log": r = a <= 0 ? null : Math.log10(a); break;
        case "sqrt": r = a < 0 ? null : Math.sqrt(a); break;
        case "abs": r = Math.abs(a); break;
        case "floor": r = Math.floor(a); break;
        case "ceil": r = Math.ceil(a); break;
        case "pow": {
          const b = second ? evaluate(second, vars) : null;
          if (b === null) r = null;
          else if (a === 0) r = b > 0 ? 0 : b === 0 ? 1 : null;
          else if (a < 0 && frac(b) !== 0) r = null;
          else if (b * Math.log(Math.abs(a)) > 700) r = null;
          else r = Math.pow(a, b);
          break;
        }
        default: r = null;
      }
      return ok(r) ? r : null;
    }
  }
}

/** A parsed formula as a function of the variables. */
export function compile(s: string, names: string[]): (vars: Record<string, number>) => number | null {
  const node = parse(s, names);
  return (vars) => evaluate(node, vars);
}
