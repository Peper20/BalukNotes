#!/usr/bin/env node
// Снимок страницы через DevTools-протокол headless Chromium — для проверки
// вида глазами. В отличие от `chromium --screenshot`, снимает то, что видно
// после прокрутки (якоря), ждёт отрисовки и умеет выполнять JS.
//
//   tools/shot.mjs <url> <out.png> [--size 1300x900] [--dark] [--full]
//                  [--wait 300] [--eval 'JS'] [--print 'JS-выражение']
//
//   --dark   prefers-color-scheme: dark
//   --full   вся страница целиком (иначе — окно)
//   --wait   запас после отрисовки, мс (отрисовку ждёт по <html data-state>)
//   --eval   выполнить перед снимком (например, открыть настройки)
//   --print  вывести значение выражения (например, прокрутку колонки #page)
//
// Много снимков разом — tools/visual.mjs.

import { Browser } from "./lib/browser.mjs";

const args = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = args.indexOf(name);
  if (i < 0) return fallback;
  const value = args[i + 1];
  args.splice(i, 2);
  return value;
};
const bool = (name) => {
  const i = args.indexOf(name);
  if (i >= 0) args.splice(i, 1);
  return i >= 0;
};
const [width, height] = flag("--size", "1300x900").split("x").map(Number);
const wait = Number(flag("--wait", "300"));
const evalJs = flag("--eval", null);
const printJs = flag("--print", null);
const dark = bool("--dark");
const full = bool("--full");
const [url, out] = args;
if (!url || !out) {
  console.error("tools/shot.mjs <url> <out.png> [--size WxH] [--dark] [--full] [--wait ms] [--eval JS] [--print JS]");
  process.exit(2);
}

const browser = await Browser.launch({ width, height });
try {
  if (dark) await browser.dark(true);
  await browser.open(url, { wait });
  if (evalJs) {
    await browser.evaluate(evalJs);
    await new Promise((r) => setTimeout(r, 300));
  }
  if (printJs) console.log(JSON.stringify(await browser.evaluate(printJs)));
  await browser.screenshot(out, { full });
} finally {
  await browser.close();
}
