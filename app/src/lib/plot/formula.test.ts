import { describe, expect, it } from "vitest";
// The fixture snapshot: it has the values computed by baluk (Typst), the reference.
import snapshot from "../../../../tests/snapshots/Рисунки/Интерактив.snap?raw";
import { compile, FormulaError, parse } from "./formula";

interface Check {
  f: string;
  vars: Record<string, number>;
  value: number | null;
}

describe("formula", () => {
  it("matches Typst on a set of formulas", () => {
    const json = /<pre class="k-plot-check">([^<]*)<\/pre>/.exec(snapshot)?.[1];
    expect(json).toBeTruthy();
    const checks = JSON.parse(json!.replaceAll("&quot;", '"')) as Check[];
    expect(checks.length).toBeGreaterThan(10);
    for (const c of checks) {
      const got = compile(c.f, ["x", "y", "a"])(c.vars);
      if (c.value === null) expect(got, c.f).toBeNull();
      else expect(got, c.f).toBeCloseTo(c.value, 9);
    }
  });

  it("unary minus binds tighter than multiplication, subtraction goes left to right", () => {
    expect(compile("-x * -2", ["x"])({ x: 3 })).toBe(6);
    expect(compile("10 - 4 - 3", [])({})).toBe(3);
    expect(compile("2 + 3 * 4", [])({})).toBe(14);
  });

  it("clear errors", () => {
    const err = (s: string) => {
      try {
        parse(s, ["x", "a"]);
      } catch (e) {
        expect(e).toBeInstanceOf(FormulaError);
        return (e as Error).message;
      }
      throw new Error("no error: " + s);
    };
    expect(err("x^2")).toContain("calc.pow");
    expect(err("sin(x)")).toContain("неизвестное имя «sin»");
    expect(err("calc.foo(x)")).toContain("нет функции");
    expect(err("calc.pow(x)")).toContain("аргументов: 2");
    expect(err("(x + 1")).toContain("«)»");
    expect(err("x x")).toContain("лишнее");
    expect(err("")).toContain("пустая");
  });
});
