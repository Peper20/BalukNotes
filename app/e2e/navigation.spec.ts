import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { VAULT, noteUrl, open, ready, title } from "./helpers";

test("главная: граф и список, клик по узлу открывает заметку", async ({ page }) => {
  await page.goto("/");
  await ready(page);
  await expect(page.locator(".home-lead")).toContainText("2 книги");
  await expect(page.locator(".graph-node")).not.toHaveCount(0);
  // По кружку: центр узла (кружок + подпись) может прийтись на щель между ними.
  await page.locator('.graph-node[data-id="Сеть/SSH"] circle').click();
  await ready(page);
  await expect(title(page)).toHaveText("SSH");
  await expect(page).toHaveURL(noteUrl("Сеть/SSH"));
});

test("ссылка в главу книги по тексту и по метке; назад и вперёд", async ({ page }) => {
  await open(page, "Особые случаи/Ссылки");
  await page.locator("#note").getByRole("link", { name: "Код из файла" }).click();
  await ready(page);
  await expect(page).toHaveURL(noteUrl("Книга", "Код-из-файла"));
  await expect(page.locator("#note h3", { hasText: "Код из файла" })).toBeInViewport();
  await expect(page.locator(".chapter-nav")).toContainText("1. Основы");

  // По метке: <особый> — тот же раздел книги.
  await page.goBack();
  await ready(page);
  await expect(title(page)).toHaveText("Ссылки");
  await page.locator("#note").getByRole("link", { name: "особый" }).click();
  await ready(page);
  await expect(page.locator("#особый")).toBeInViewport();

  await page.goBack();
  await ready(page);
  await page.goForward();
  await ready(page);
  await expect(page.locator("#особый")).toBeInViewport();
});

test("главы книги: следующая, предыдущая, оглавление ведёт в нужную главу", async ({ page }) => {
  await open(page, "Книга");
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Основы/);
  await page.locator(".chapter-nav a.next").click();
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Продолжение/);
  await expect(page.locator("#Итоги-2")).toBeAttached();
  await expect(page.locator("#Итоги")).not.toBeAttached();
  await page.locator(".chapter-nav a.prev").click();
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Основы/);
});

test("книгу поправили — переход к главе показывает новую версию, а не запас", async ({ page }) => {
  const file = join(VAULT, "Книга", "02-продолжение.typ");
  const before = readFileSync(file, "utf8");
  // «Только по кнопке»: событие о правке заметку само не перезагрузит.
  const mode = async (value: "auto" | "manual") =>
    expect((await page.request.put("/api/settings", { data: { "refresh.mode": value } })).ok()).toBe(true);
  try {
    await mode("manual");
    const prefetched = page.waitForResponse((r) => decodeURIComponent(r.url()).endsWith("/api/notes/Книга?chapter=1"));
    await open(page, "Книга");
    await prefetched;
    appendFileSync(file, "\nПравка второй главы.\n");
    await page.keyboard.press("BracketRight");
    await ready(page);
    await expect(page.locator("#note h2.k-h1")).toHaveText(/Продолжение/);
    await expect(page.locator("#note")).toContainText("Правка второй главы.");
    await expect(page.locator("#status")).toContainText("книга обновлена");
  } finally {
    writeFileSync(file, before);
    await mode("auto");
  }
});

test("Ctrl+F в книге по главам — поиск по всей книге с номером главы; повторное — браузеру", async ({ page }) => {
  await open(page, "Книга");
  await page.keyboard.press("Control+KeyF");
  const palette = page.locator(".palette");
  await expect(palette.locator("input")).toHaveAttribute("placeholder", /книги «/);
  await palette.locator("input").fill("Итоги");
  // «Итоги» есть в каждой главе — различаются номером главы.
  const chapters = palette.locator(".p-chapter");
  for (const n of [1, 2, 3]) await expect(chapters.filter({ hasText: `гл. ${n}` }).first()).toBeVisible();
  await palette.locator("input").press("Control+KeyF");
  await expect(palette).toBeHidden();
  // Выбор результата — переход в главу.
  await page.keyboard.press("Control+KeyF");
  await palette.locator("input").fill("Итоги");
  await expect(chapters.first()).toBeVisible();
  await palette.locator(".palette-list li[role=option]", { has: page.locator(".p-chapter", { hasText: "гл. 2" }) }).click();
  await ready(page);
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Продолжение/);
  await expect(page.locator("#Итоги-2")).toBeInViewport();
});

test("главу книги отдаёт сервер: в странице одна глава, перезагрузка — та же глава", async ({ page }) => {
  const requested: string[] = [];
  page.on("request", (r) => {
    const url = new URL(r.url());
    if (url.pathname.startsWith("/api/notes/")) requested.push(decodeURIComponent(url.pathname + url.search));
  });
  await open(page, "Книга");
  await expect(page.locator("#note h2.k-h1")).toHaveCount(1);
  await expect(page.locator("#note .k-title")).toBeAttached();
  await page.keyboard.press("BracketRight");
  await ready(page);
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Продолжение/);
  await expect(page.locator("#note h2.k-h1")).toHaveCount(1);
  await expect(page.locator("#note .k-title")).not.toBeAttached();
  expect(requested).toContain("/api/notes/Книга?chapter=1");

  // Оглавление — всей книги; пункт из другой главы грузит её.
  await page.locator(".toc a", { hasText: "Картинка из файла" }).first().dispatchEvent("click");
  await ready(page);
  await expect(page.locator("#Картинка-из-файла")).toBeInViewport();

  await page.reload();
  await ready(page);
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Приложение/);
  expect(requested.at(-1)).toBe("/api/notes/Книга?anchor=Картинка-из-файла");
});

test("соседние главы — заранее: переход без запроса к серверу", async ({ page }) => {
  const requested: string[] = [];
  page.on("request", (r) => {
    const url = new URL(r.url());
    if (url.pathname.startsWith("/api/notes/")) requested.push(decodeURIComponent(url.pathname + url.search));
  });
  const prefetched = page.waitForResponse((r) => decodeURIComponent(r.url()).endsWith("/api/notes/Книга?chapter=1"));
  await open(page, "Книга");
  // Ответ пришёл целиком и разобран клиентом (запас пополняется после разбора).
  await (await prefetched).finished();
  await page.waitForTimeout(200);
  const before = requested.length;
  await page.keyboard.press("BracketRight");
  await ready(page);
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Продолжение/);
  expect(requested.slice(before)).not.toContain("/api/notes/Книга?chapter=1");
  // И назад — тоже из запаса (показанная глава остаётся соседкой).
  await page.keyboard.press("BracketLeft");
  await ready(page);
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Основы/);
  expect(requested.slice(before)).not.toContain("/api/notes/Книга?chapter=0");
});

test("имена с + # % и глубокая вложенность", async ({ page }) => {
  await open(page, "Особые случаи/Ссылки");
  for (const name of ["C++ и C#", "50% готово", "Дно"]) {
    await page.locator("#note").getByRole("link", { name, exact: true }).click();
    await ready(page);
    await expect(title(page)).toHaveText(name);
    await page.reload();
    await ready(page);
    await expect(title(page)).toHaveText(name);
    await page.goBack();
    await ready(page);
  }
  await expect(page.locator('#tree a[data-id="Глубоко/а/б/в/г/Дно"]')).toBeVisible();
});

test("битая ссылка не ведёт никуда", async ({ page }) => {
  await open(page, "Особые случаи/Ссылки");
  const broken = page.locator('#note a.k-link[data-k-target="Нет/Такой заметки"]');
  await expect(broken).not.toHaveAttribute("href");
});

test("ошибка и предупреждения сборки", async ({ page }) => {
  await open(page, "Особые случаи/Ошибка компиляции");
  await expect(page.locator("#problems .errors")).toContainText("unknown variable: no-such-function");
  await open(page, "Особые случаи/Предупреждение");
  await expect(page.locator("#problems summary")).toHaveText("Предупреждения: 4");
});

test("несуществующая заметка", async ({ page }) => {
  await open(page, "Нет/Такой");
  await expect(page.locator("#note")).toContainText("Заметки «Нет/Такой» нет.");
});

test("ссылаются сюда", async ({ page }) => {
  await open(page, "Сеть/SSH");
  const bl = page.locator("#backlinks");
  await expect(bl).toContainText("UFW");
  await expect(bl).toContainText("Ссылки");
  await bl.getByRole("link", { name: "UFW" }).click();
  await ready(page);
  await expect(title(page)).toHaveText("UFW");
});
