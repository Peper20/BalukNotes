// Сборка `assets/baluk.css` из файлов блоков (`baluk.css` — их порядок):
// `@import "./…";` заменяется содержимым файла, `lucide("имя", толщина)` —
// значком из пакета `@lucide/icons` (data-URL для CSS-маски). Работает в
// Node — из плагина Vite (`plugin.ts`), не в браузере.

import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Узел значка Lucide: `[тег, атрибуты, дети?]`. */
type IconNode = [string, Record<string, string | number>, IconNode[]?];

/** Файл-порядок блоков. */
export const ENTRY = resolve(dirname(fileURLToPath(import.meta.url)), "baluk.css");

const IMPORT = /^@import\s+"([^"]+)";[ \t]*$/gm;
const LUCIDE = /lucide\(\s*"([a-z0-9-]+)"\s*,\s*([\d.]+)\s*\)/g;

/** Значок Lucide `name` — `url("data:image/svg+xml,…")` для `mask`. */
export async function lucideUrl(name: string, strokeWidth: number | string): Promise<string> {
  const mod = (await import(/* @vite-ignore */ `@lucide/icons/icons/${name}`)) as { default: { node: IconNode[] } };
  return svgUrl(mod.default.node, strokeWidth);
}

/** SVG значка (24×24, контур `black` — маске важна только непрозрачность). */
export function svgUrl(nodes: IconNode[], strokeWidth: number | string): string {
  const el = ([tag, attrs, children]: IconNode): string => {
    const a = Object.entries(attrs)
      .filter(([k]) => k !== "key")
      .map(([k, v]) => ` ${k}='${v}'`)
      .join("");
    return children?.length ? `<${tag}${a}>${children.map(el).join("")}</${tag}>` : `<${tag}${a}/>`;
  };
  const svg =
    `<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='${strokeWidth}'` +
    ` stroke-linecap='round' stroke-linejoin='round'>${nodes.map(el).join("")}</svg>`;
  return `url("data:image/svg+xml,${svg.replace(/[<>#%"]/g, encodeURIComponent)}")`;
}

async function replaceAsync(s: string, re: RegExp, fn: (...m: string[]) => Promise<string>): Promise<string> {
  const parts = await Promise.all([...s.matchAll(re)].map((m) => fn(...m)));
  let i = 0;
  return s.replace(re, () => parts[i++]!);
}

/**
 * Склеить файл `file` с его `@import` (рекурсивно); `seen` — прочитанные
 * файлы (для слежения в режиме разработки).
 */
export async function bundle(file = ENTRY, seen: Set<string> = new Set()): Promise<string> {
  if (seen.has(file)) throw new Error(`baluk.css: ${file} подключён дважды`);
  seen.add(file);
  const text = await readFile(file, "utf8");
  const inlined = await replaceAsync(text, IMPORT, async (_, path) => {
    const body = await bundle(resolve(dirname(file), path!), seen);
    return body.trimEnd();
  });
  return replaceAsync(inlined, LUCIDE, (_, name, width) => lucideUrl(name!, width!));
}
