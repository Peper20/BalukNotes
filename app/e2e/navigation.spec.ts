import { expect, test } from "@playwright/test";
import { noteUrl, open, ready, title } from "./helpers";

test("главная: граф и список, клик по узлу открывает заметку", async ({ page }) => {
  await page.goto("/");
  await ready(page);
  await expect(page.locator(".home-lead")).toContainText("1 книга");
  await expect(page.locator(".graph-node")).not.toHaveCount(0);
  await page.locator('.graph-node[data-id="Сеть/SSH"]').click();
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
  await expect(page.locator("#problems .errors")).toContainText("unknown variable: нет-такой-функции");
  await open(page, "Особые случаи/Предупреждение");
  await expect(page.locator("#problems summary")).toHaveText("Предупреждения: 2");
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
