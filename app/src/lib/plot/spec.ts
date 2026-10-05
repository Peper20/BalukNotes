// The description of an interactive figure from `data-k-plot` (written by baluk/plots.typ).

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
  /** Theme color name: line, second, third, accent. */
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
  /** Plot area size, cm. */
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
  /** Initial view: rotation and tilt, degrees. */
  view: [number, number];
  /** Box size, cm. */
  size: number;
}

export type PlotSpec = Plot2D | Plot3D;

/** The figure description from the attribute; not parsed - null (the frame stays). */
export function readSpec(el: Element): PlotSpec | null {
  try {
    const spec = JSON.parse(el.getAttribute("data-k-plot") ?? "") as PlotSpec;
    return spec.kind === "2d" || spec.kind === "3d" ? spec : null;
  } catch {
    return null;
  }
}

/** Whether all formulas of the figure parse (otherwise the client keeps the Typst frame). */
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

/** CSS variable of a figure color by the theme color name. */
export function colorVar(name: string): string {
  const known = ["line", "second", "third", "face"];
  return name === "accent" ? "--k-accent" : `--k-fig-${known.includes(name) ? name : "line"}`;
}

/** Initial parameter values. */
export function defaults(params: Param[]): Record<string, number> {
  return Object.fromEntries(params.map((p) => [p.name, p.value]));
}

/** cm -> pt (the unit of the viewBox and of the label font size, as in the Typst frame). */
export const PT_PER_CM = 72 / 2.54;
/** Font size of the PDF body text and the figure labels, pt (theme.typ). */
export const TEXT_PT = 11;
export const SMALL_PT = 9.2;
