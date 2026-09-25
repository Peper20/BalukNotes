#!/usr/bin/env node
// Снимок страницы через DevTools-протокол headless Chromium — для проверки
// вида глазами. В отличие от `chromium --screenshot`, снимает то, что видно
// после прокрутки (якоря), умеет ждать отрисовки и выполнять JS.
//
//   tools/shot.mjs <url> <out.png> [--size 1300x900] [--dark] [--full]
//                  [--wait 1500] [--eval 'JS'] [--print 'JS-выражение']
//
//   --dark   prefers-color-scheme: dark
//   --full   вся страница целиком (иначе — окно)
//   --eval   выполнить перед снимком (например, открыть настройки)
//   --print  вывести значение выражения (например, scrollY)
//
// Нужны chromium и Node ≥ 22 (встроенный WebSocket).

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

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
const wait = Number(flag("--wait", "1500"));
const evalJs = flag("--eval", null);
const printJs = flag("--print", null);
const dark = bool("--dark");
const full = bool("--full");
const [url, out] = args;
if (!url || !out) {
  console.error("tools/shot.mjs <url> <out.png> [--size WxH] [--dark] [--full] [--wait ms] [--eval JS] [--print JS]");
  process.exit(2);
}

const profile = mkdtempSync(join(tmpdir(), "shot-"));
const browser = spawn("chromium", [
  "--headless=new", "--disable-gpu", "--hide-scrollbars", "--remote-debugging-port=0",
  `--user-data-dir=${profile}`, `--window-size=${width},${height}`, "about:blank",
], { stdio: ["ignore", "ignore", "pipe"] });

const endpoint = await new Promise((resolve, reject) => {
  let log = "";
  browser.stderr.on("data", (d) => {
    log += d;
    const m = log.match(/DevTools listening on (ws:\/\/\S+)/);
    if (m) resolve(m[1]);
  });
  browser.on("exit", () => reject(new Error(`chromium завершился:\n${log}`)));
});

const base = endpoint.replace(/^ws/, "http").replace(/\/devtools\/browser\/.*/, "");
const target = (await (await fetch(`${base}/json/list`)).json()).find((t) => t.type === "page");
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r, { once: true }));

let id = 0;
const pending = new Map();
ws.addEventListener("message", (e) => {
  const msg = JSON.parse(e.data);
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  }
});
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const n = ++id;
  pending.set(n, (m) => (m.error ? reject(new Error(`${method}: ${m.error.message}`)) : resolve(m.result)));
  ws.send(JSON.stringify({ id: n, method, params }));
});
const evaluate = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
  return r.result.value;
};

try {
  await send("Page.enable");
  // mobile: false — эмуляция телефона масштабирует окно (innerHeight
  // вырастает вдвое) и искажает замеры; узкую вёрстку проверяем шириной.
  await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  if (dark) await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "dark" }] });
  await send("Page.navigate", { url });
  // Ждём загрузки и того, что клиент дорисует заметку (fetch + вставка).
  await evaluate(`new Promise(r => document.readyState === "complete" ? r() : addEventListener("load", r))`);
  await evaluate(`document.fonts.ready.then(() => true)`);
  await new Promise((r) => setTimeout(r, wait));
  if (evalJs) {
    await evaluate(evalJs);
    await new Promise((r) => setTimeout(r, 300));
  }
  if (printJs) console.log(JSON.stringify(await evaluate(printJs)));
  const shot = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: full });
  writeFileSync(out, Buffer.from(shot.data, "base64"));
} finally {
  ws.close();
  browser.kill();
  rmSync(profile, { recursive: true, force: true });
}
