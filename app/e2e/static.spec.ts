// Статический сайт (notes build) с диска, без сервера: кадры с ползунком,
// живой рисунок, оглавление и «Ссылаются сюда» (вшиты при сборке), тема,
// части скрипта по требованию, поиск, граф и теги.
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

test("сайт: заголовок с формулой в оглавлении — формулой", async ({ page }) => {
  await page.goto(pageUrl("Формулы и теги"));
  const item = page.locator(".k-static-toc a", { hasText: "Пространство" });
  await expect(item.locator("math")).toHaveCount(2);
  await expect(item.locator("strong")).toHaveText("норма");
});

test("сайт: поиск по заметкам без сервера — кнопка и «/», переход к разделу", async ({ page }) => {
  await page.goto(pageUrl("index"));
  await page.locator("#k-search").click();
  const input = page.locator(".k-search-input");
  await expect(input).toBeFocused();
  await input.fill("смена порта");
  const first = page.locator(".k-search-row").first();
  await expect(first.locator(".k-search-title")).toHaveText("SSH · Смена порта");
  await expect(first.locator("mark").first()).toHaveText("Смена");
  await input.press("Enter");
  await expect(page).toHaveURL(/SSH\.html#/);
  await expect(page.locator("main h2", { hasText: "Смена порта" })).toBeInViewport();

  // «/» открывает поиск, Esc закрывает; поиск — как у ядра («ё» = «е», регистр).
  await page.keyboard.press("/");
  await expect(input).toBeFocused();
  await input.fill("НЕТ-ТАКОГО-СЛОВА");
  await expect(page.locator(".k-search-empty")).toHaveText("Ничего не нашлось.");
  await input.press("Escape");
  await expect(page.locator(".k-search")).toHaveCount(0);
});

test("сайт: граф заметок — узлы из сборки, щелчок открывает заметку", async ({ page }) => {
  await page.goto(pageUrl("graph"));
  const node = page.locator('.graph-node[data-id="Сеть/SSH"]');
  await expect(node).toBeVisible();
  await expect(page.locator(".k-site-graph-count")).toContainText("узл");
  // Заметка с ошибкой сборки — без страницы: узел как у ненаписанной.
  await expect(page.locator('.graph-node[data-id="Особые случаи/Ошибка компиляции"]')).toHaveClass(/missing/);
  await page.locator(".k-site-graph-search").fill("ufw");
  await expect(page.locator(".k-site-graph-count")).toContainText("найдено 1");
  await node.locator("circle").click();
  await expect(page).toHaveURL(/SSH\.html$/);
});

test("сайт: теги — ссылки из шапки на страницу тегов", async ({ page }) => {
  await page.goto(pageUrl("Формулы и теги"));
  await page.locator(".k-tags a", { hasText: "теги с пробелом" }).click();
  await expect(page).toHaveURL(/tags\.html#/);
  const section = page.locator(".k-site-tag-notes", { has: page.locator("h2", { hasText: "#теги с пробелом" }) });
  await expect(section).toBeInViewport();
  await section.getByRole("link", { name: "Формулы и теги" }).click();
  await expect(page).toHaveURL(pageUrl("Формулы и теги"));
});

test("сайт: на узком экране панель, граф и теги не шире окна", async ({ page }) => {
  await page.setViewportSize({ width: 400, height: 800 });
  for (const id of ["Сеть/SSH", "graph", "tags", "index"]) {
    await page.goto(pageUrl(id));
    await page.evaluate(() => document.fonts.ready);
    expect(await page.evaluate(() => document.documentElement.scrollWidth), id).toBeLessThanOrEqual(400);
  }
});
