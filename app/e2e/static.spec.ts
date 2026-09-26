// Статический сайт (notes build) с диска, без сервера: кадры с ползунком,
// живой рисунок, оглавление и «Ссылаются сюда» (вшиты при сборке), тема.
import { execFileSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { expect, test } from "@playwright/test";

const repo = fileURLToPath(new URL("../../", import.meta.url));
const site = `${repo}tests/.data/e2e-site/`;
const pageUrl = (id: string) => pathToFileURL(`${site}${id}.html`).href;

test.beforeAll(() => {
  // В фикстуре есть заметка с ошибкой — сборка кончается ошибкой, остальное собрано.
  try {
    // Данные (кэш сборки) — в tests/.data, не в data/ пользователя.
    const data = `${repo}tests/.data/e2e-site-data`;
    execFileSync("cargo", ["run", "-q", "-p", "notes-cli", "--", "--data", data, "--vault", "tests/vault", "build", site], { cwd: repo, stdio: "pipe" });
  } catch {
    // проверяем то, что собралось
  }
});

test("сайт: кадры с ползунком и живой рисунок без сервера", async ({ page }) => {
  await page.goto(pageUrl("Рисунки/Кадры"));
  const frames = page.locator(".k-frames").first();
  await expect(frames).toHaveAttribute("data-live", "");
  await expect(frames.locator(".k-frames-count")).toHaveText("4 / 12");
  await frames.locator(".k-frames-next").click();
  await expect(frames.locator(".k-frames-item[data-current] .k-frames-label")).toHaveText("n = 5");

  await page.goto(pageUrl("Рисунки/Интерактив"));
  await expect(page.locator(".k-plot[data-live]").first()).toBeVisible();
});

test("сайт: оглавление сбоку ведёт к разделу, «Ссылаются сюда», тема", async ({ page }) => {
  await page.goto(pageUrl("Сеть/SSH"));
  const toc = page.locator(".k-static-toc");
  await expect(toc).toHaveAttribute("open", "");
  await toc.getByRole("link", { name: "Смена порта" }).click();
  await expect(page.locator("#note, main").locator("h2", { hasText: "Смена порта" })).toBeInViewport();
  await expect(toc.locator("a.current")).toHaveText("Смена порта");

  const back = page.locator(".k-static-backlinks");
  await expect(back).toContainText("Ссылаются сюда");
  await back.getByRole("link", { name: "UFW" }).click();
  await expect(page).toHaveURL(/UFW\.html$/);

  const theme = page.locator("#k-theme");
  const before = await page.locator("html").getAttribute("data-theme");
  await theme.click();
  expect(await page.locator("html").getAttribute("data-theme")).not.toBe(before);
});

test("сайт: на узком экране оглавление свёрнуто и сворачивается после перехода", async ({ page }) => {
  await page.setViewportSize({ width: 400, height: 800 });
  await page.goto(pageUrl("Сеть/SSH"));
  const toc = page.locator(".k-static-toc");
  await expect(toc).not.toHaveAttribute("open", "");
  await toc.locator("summary").click();
  await toc.getByRole("link", { name: "Смена порта" }).click();
  await expect(toc).not.toHaveAttribute("open", "");
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(400);
});

test("сайт: живые блоки — отдельной частью, только где они есть", async ({ page }) => {
  const parts: string[] = [];
  page.on("request", (r) => {
    const m = /assets\/(static[^/]*\.js)$/.exec(r.url());
    if (m) parts.push(m[1]!);
  });
  await page.goto(pageUrl("Сеть/SSH"));
  await expect(page.locator(".k-static-toc a").first()).toBeVisible();
  expect(parts).toEqual(["static.js"]);

  parts.length = 0;
  await page.goto(pageUrl("Рисунки/Граф хранилища"));
  await expect(page.locator(".k-graph[data-live] .graph-svg").first()).toBeVisible();
  expect(parts).toEqual(["static.js", "static-live.js"]);
});
