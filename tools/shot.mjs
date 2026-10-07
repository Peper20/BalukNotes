#!/usr/bin/env node
// A page screenshot via the DevTools protocol of headless Chromium - for
// checking the look by eye. Unlike `chromium --screenshot`, it captures what
// is visible after scrolling (anchors), waits for rendering and can run JS.
//
//   tools/shot.mjs <url> <out.png> [--size 1300x900] [--dark] [--full]
//                  [--wait 300] [--eval 'JS'] [--print 'JS expression']
//
//   --dark   prefers-color-scheme: dark
//   --full   the whole page (otherwise the viewport)
//   --wait   extra time after rendering, ms (rendering is awaited by <html data-state>)
//   --eval   run before the screenshot (e.g. open the settings)
//   --print  print the value of an expression (e.g. the scroll of the #page column)
//
// Many screenshots at once - tools/visual.mjs.

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
