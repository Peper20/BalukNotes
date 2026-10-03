// Сборка `assets/baluk.css` из файлов блоков (`baluk.css` — их порядок):
// `@import "./…";` заменяется содержимым файла, затем `forBrowsers`. Работает
// в Node — из плагина Vite (`plugin.ts`), не в браузере.

import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { transform } from "lightningcss";

/** Файл-порядок блоков. */
export const ENTRY = resolve(dirname(fileURLToPath(import.meta.url)), "baluk.css");

const IMPORT = /^@import\s+"([^"]+)";[ \t]*$/gm;

/**
 * Склеить файл `file` с его `@import` (рекурсивно); `seen` — прочитанные
 * файлы (для слежения в режиме разработки).
 */
export async function bundle(file = ENTRY, seen: Set<string> = new Set()): Promise<string> {
  if (seen.has(file)) throw new Error(`baluk.css: ${file} подключён дважды`);
  seen.add(file);
  const text = await readFile(file, "utf8");
  const parts = await Promise.all([...text.matchAll(IMPORT)].map((m) => bundle(resolve(dirname(file), m[1]!), seen)));
  let i = 0;
  return text.replace(IMPORT, () => parts[i++]!.trimEnd());
}

/**
 * Браузеры, под которые дописываются префиксы: первые с вложенностью CSS (она
 * в блоках уже есть - разворачивать её не нужно). Окно приложения - WebKitGTK:
 * без `-webkit-user-select` граф выделялся целиком при протяжке.
 */
const TARGETS = { chrome: 120 << 16, firefox: 117 << 16, safari: (17 << 16) | (2 << 8) };

/** Склейка -> CSS для браузеров: префиксы (`-webkit-user-select`...), без сжатия. */
export function forBrowsers(css: string): string {
  const out = transform({ filename: "baluk.css", code: new TextEncoder().encode(css), targets: TARGETS });
  return new TextDecoder().decode(out.code);
}
