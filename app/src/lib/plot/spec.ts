// Описание интерактивного рисунка из `data-k-plot` (пишет baluk/plots.typ).

import { compile } from "./formula";

export interface Param {
  name: string;
  min: number;
  max: number;
  step: number;
  value: number;
}

export interface Curve {
  f: string;
  /** Имя цвета темы: line, second, third, accent. */
  color: string;
  label: string | null;
  dashed: boolean;
}

export interface Plot2D {
  kind: "2d";
  curves: Curve[];
  x: [number, number];
  y: [number, number];
  params: Param[];
  labels: [string, string];
  /** Размер поля графика, см. */
  width: number;
  height: number;
}

export interface Plot3D {
  kind: "3d";
  f: string;
  x: [number, number];
  y: [number, number];
  z: [number, number];
  params: Param[];
  labels: [string, string, string];
  n: number;
  style: "shaded" | "wire";
  color: string;
  /** Начальный вид: поворот и наклон, градусы. */
  view: [number, number];
  /** Размер коробки, см. */
  size: number;
}

export type PlotSpec = Plot2D | Plot3D;

/** Описание рисунка из атрибута; не разобралось — null (останется кадр). */
export function readSpec(el: Element): PlotSpec | null {
  try {
    const spec = JSON.parse(el.getAttribute("data-k-plot") ?? "") as PlotSpec;
    return spec.kind === "2d" || spec.kind === "3d" ? spec : null;
  } catch {
    return null;
  }
}

/** Разбираются ли все формулы рисунка (иначе клиент оставляет кадр Typst). */
export function formulasOk(spec: PlotSpec): boolean {
  const names = spec.params.map((p) => p.name);
  try {
    if (spec.kind === "2d") for (const c of spec.curves) compile(c.f, ["x", ...names]);
    else compile(spec.f, ["x", "y", ...names]);
    return true;
  } catch {
    return false;
  }
}

/** CSS-переменная цвета рисунка по имени цвета темы. */
export function colorVar(name: string): string {
  const known = ["line", "second", "third", "face"];
  return name === "accent" ? "--k-accent" : `--k-fig-${known.includes(name) ? name : "line"}`;
}

/** Начальные значения параметров. */
export function defaults(params: Param[]): Record<string, number> {
  return Object.fromEntries(params.map((p) => [p.name, p.value]));
}

/** см → pt (единица viewBox и кегля подписей, как в кадре Typst). */
export const PT_PER_CM = 72 / 2.54;
/** Кегль основного текста PDF и подписей рисунка, pt (theme.typ). */
export const TEXT_PT = 11;
export const SMALL_PT = 9.2;
