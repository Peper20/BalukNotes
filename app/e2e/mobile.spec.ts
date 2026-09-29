// Узкий экран (телефон): ничего не шире окна, всплывающие панели работают.
import { expect, test, type Page } from "@playwright/test";
import { noteUrl, open, ready, vaultUrl } from "./helpers";

test.use({ viewport: { width: 400, height: 800 }, hasTouch: true });

/** Страница не прокручивается вбок (широкое прокручивается внутри себя). */
async function noSideScroll(page: Page) {
  // Ширина формул — после загрузки шрифтов.
  const overflow = await page.evaluate(async () => {
    await document.fonts.ready;
    return document.documentElement.scrollWidth - innerWidth;
  });
  expect(overflow).toBeLessThanOrEqual(0);
}

test("узкий экран: страницы и настройки не шире окна", async ({ page }) => {
  for (const url of [vaultUrl("/"), vaultUrl("/tags"), vaultUrl("/graph"), noteUrl("Формулы и теги"), noteUrl("Книга"), noteUrl("Сеть/SSH"), noteUrl("Рисунки/Интерактив"), noteUrl("Рисунки/Кадры")]) {
    await page.goto(url);
    await ready(page);
    await noSideScroll(page);
  }
  await page.locator("#open-settings").click();
  const settings = page.locator("#settings");
  await expect(settings).toBeVisible();
  // Содержимое диалога не шире его самого (иначе списки уходят за край).
  expect(await settings.evaluate((d) => d.scrollWidth - d.clientWidth)).toBeLessThanOrEqual(0);
});

test("узкий экран: оглавление всплывает, ведёт в другую главу и прячется", async ({ page }) => {
  await open(page, "Книга");
  const toc = page.locator(".toc");
  await expect(toc).not.toHaveClass(/open/);
  await page.locator("#toggle-toc").click();
  await expect(toc).toHaveClass(/open/);
  await toc.getByRole("link", { name: "Картинка из файла" }).click();
  await ready(page);
  await expect(page.locator("#Картинка-из-файла")).toBeInViewport();
  await expect(toc).not.toHaveClass(/open/);
});

test("узкий экран: активная вкладка видна целиком, палитра — в окне", async ({ page }) => {
  await open(page, "Сеть/SSH");
  for (const id of ["Книга", "Формулы и теги"]) {
    await page.keyboard.press("Alt+KeyT");
    await ready(page);
    await page.goto(noteUrl(id));
    await ready(page);
  }
  const [bar, tab] = await Promise.all([page.locator(".tabbar").boundingBox(), page.locator(".tab.active").boundingBox()]);
  expect(tab!.x).toBeGreaterThanOrEqual(bar!.x);
  expect(tab!.x + tab!.width).toBeLessThanOrEqual(bar!.x + bar!.width);
  await noSideScroll(page);

  await page.locator("#open-palette").click();
  const palette = page.locator(".palette");
  await expect(palette).toBeVisible();
  const box = (await palette.boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(400);
});

test("узкий экран: выбор хранилища, меню хранилищ и диалоги не шире окна", async ({ page }) => {
  // Адрес без хранилища в новом браузере — экран выбора.
  await page.goto("/");
  await expect(page.locator("#vault-picker")).toBeVisible();
  await ready(page);
  await noSideScroll(page);
  await open(page, "Сеть/SSH");
  await page.locator(".topbar button.icon").first().click();
  await page.locator("#vault-switch").click();
  const menu = page.locator("#vault-menu");
  await expect(menu).toBeVisible();
  const box = (await menu.boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(400);
  await menu.getByRole("menuitem", { name: "Новое хранилище…" }).click();
  const dialog = page.locator("#vault-new");
  await expect(dialog).toBeVisible();
  expect(await dialog.evaluate((d) => d.scrollWidth - d.clientWidth)).toBeLessThanOrEqual(0);
  await dialog.getByRole("button", { name: "Отмена" }).click();
  await noSideScroll(page);
});
