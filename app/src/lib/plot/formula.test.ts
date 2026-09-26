import { describe, expect, it } from "vitest";
// Снимок фикстуры: в нём значения, посчитанные baluk (Typst) — эталон.
import snapshot from "../../../../tests/snapshots/Рисунки/Интерактив.snap?raw";
import { compile, FormulaError, parse } from "./formula";

interface Check {
  f: string;
  vars: Record<string, number>;
  value: number | null;
}

describe("формула", () => {
  it("совпадает с Typst на наборе формул", () => {
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

  it("унарный минус сильнее умножения, вычитание — слева направо", () => {
    expect(compile("-x * -2", ["x"])({ x: 3 })).toBe(6);
    expect(compile("10 - 4 - 3", [])({})).toBe(3);
    expect(compile("2 + 3 * 4", [])({})).toBe(14);
  });

  it("понятные ошибки", () => {
    const err = (s: string) => {
      try {
        parse(s, ["x", "a"]);
      } catch (e) {
        expect(e).toBeInstanceOf(FormulaError);
        return (e as Error).message;
      }
      throw new Error("нет ошибки: " + s);
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
