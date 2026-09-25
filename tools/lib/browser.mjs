// Headless Chromium через DevTools-протокол — общая часть tools/shot.mjs и
// tools/visual.mjs. Без зависимостей: нужен системный chromium и Node ≥ 22
// (встроенный WebSocket). Один браузер — на много снимков.

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export class Browser {
  static async launch({ width = 1300, height = 900 } = {}) {
    const profile = mkdtempSync(join(tmpdir(), "shot-"));
    const proc = spawn("chromium", [
      "--headless=new", "--disable-gpu", "--hide-scrollbars", "--remote-debugging-port=0",
      `--user-data-dir=${profile}`, `--window-size=${width},${height}`, "about:blank",
    ], { stdio: ["ignore", "ignore", "pipe"] });
    const endpoint = await new Promise((resolve, reject) => {
      let log = "";
      proc.stderr.on("data", (d) => {
        log += d;
        const m = log.match(/DevTools listening on (ws:\/\/\S+)/);
        if (m) resolve(m[1]);
      });
      proc.on("exit", () => reject(new Error(`chromium завершился:\n${log}`)));
    });
    const base = endpoint.replace(/^ws/, "http").replace(/\/devtools\/browser\/.*/, "");
    const target = (await (await fetch(`${base}/json/list`)).json()).find((t) => t.type === "page");
    const ws = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((r) => ws.addEventListener("open", r, { once: true }));
    const b = new Browser(proc, profile, ws);
    await b.send("Page.enable");
    await b.viewport(width, height);
    return b;
  }

  constructor(proc, profile, ws) {
    this.proc = proc;
    this.profile = profile;
    this.ws = ws;
    this.id = 0;
    this.pending = new Map();
    ws.addEventListener("message", (e) => {
      const msg = JSON.parse(e.data);
      if (msg.id && this.pending.has(msg.id)) {
        this.pending.get(msg.id)(msg);
        this.pending.delete(msg.id);
      }
    });
  }

  send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const n = ++this.id;
      this.pending.set(n, (m) => (m.error ? reject(new Error(`${method}: ${m.error.message}`)) : resolve(m.result)));
      this.ws.send(JSON.stringify({ id: n, method, params }));
    });
  }

  async evaluate(expression) {
    const r = await this.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
    return r.result.value;
  }

  // mobile: false — эмуляция телефона масштабирует окно (innerHeight
  // вырастает вдвое) и искажает замеры; узкую вёрстку проверяем шириной.
  viewport(width, height) {
    return this.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  }

  dark(on) {
    return this.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: on ? "dark" : "light" }] });
  }

  /**
   * Открыть адрес и дождаться отрисовки: `<html data-state="ready">` (если
   * клиент его ставит), затем шрифтов; `wait` — запас на дорисовку.
   */
  async open(url, { wait = 300, timeout = 60000 } = {}) {
    await this.send("Page.navigate", { url });
    await this.evaluate(`new Promise(r => document.readyState === "complete" ? r() : addEventListener("load", r))`);
    const started = Date.now();
    await sleep(50);
    while (Date.now() - started < timeout) {
      const s = await this.evaluate(`document.documentElement.dataset.state ?? "none"`);
      if (s !== "loading") break;
      await sleep(100);
    }
    await this.evaluate(`document.fonts.ready.then(() => true)`);
    await sleep(wait);
  }

  async screenshot(out, { full = false } = {}) {
    const shot = await this.send("Page.captureScreenshot", { format: "png", captureBeyondViewport: full });
    writeFileSync(out, Buffer.from(shot.data, "base64"));
  }

  async close() {
    this.ws.close();
    // Профиль удаляем после выхода браузера: до этого он ещё пишет в каталог.
    const exited = new Promise((r) => this.proc.once("exit", r));
    this.proc.kill();
    await exited;
    rmSync(this.profile, { recursive: true, force: true, maxRetries: 3 });
  }
}
