// A narrow screen (a phone): nothing wider than the window, the popup panels work.
import { expect, test, type Page } from "@playwright/test";
import { noteUrl, open, ready, vaultUrl } from "./helpers";

test.use({ viewport: { width: 400, height: 800 }, hasTouch: true });

/** The page does not scroll sideways (wide things scroll inside themselves). */
async function noSideScroll(page: Page) {
  // Formula widths after the fonts load.
  const overflow = await page.evaluate(async () => {
    await document.fonts.ready;
    const column = document.getElementById("page"); // it scrolls, not the document
    const inner = column ? column.scrollWidth - column.clientWidth : 0;
    return Math.max(document.documentElement.scrollWidth - innerWidth, inner);
  });
  expect(overflow).toBeLessThanOrEqual(0);
}

test("narrow screen: pages and settings are not wider than the window", async ({ page }) => {
  for (const url of [vaultUrl("/"), vaultUrl("/tags"), vaultUrl("/graph"), noteUrl("Формулы и теги"), noteUrl("Книга"), noteUrl("Сеть/SSH"), noteUrl("Рисунки/Интерактив"), noteUrl("Рисунки/Кадры")]) {
    await page.goto(url);
    await ready(page);
    await noSideScroll(page);
  }
  await page.locator("#open-settings").click();
  const settings = page.locator("#settings");
  await expect(settings).toBeVisible();
  // The dialog content is not wider than the dialog (otherwise lists go past the edge).
  expect(await settings.evaluate((d) => d.scrollWidth - d.clientWidth)).toBeLessThanOrEqual(0);
});

test("narrow screen: the outline pops up, leads to another chapter and hides", async ({ page }) => {
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

test("narrow screen: the active tab is fully visible, the palette fits the window", async ({ page }) => {
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

test("narrow screen: the vault picker, the vault menu and dialogs are not wider than the window", async ({ page }) => {
  // An address without a vault in a new browser: the picker screen.
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
  // The menu is above the switcher at the bottom of the panel, within the window.
  expect(box.y).toBeGreaterThanOrEqual(0);
  expect(box.y + box.height).toBeLessThanOrEqual(800);
  await menu.getByRole("menuitem", { name: "Новое хранилище…" }).click();
  const dialog = page.locator("#vault-new");
  await expect(dialog).toBeVisible();
  expect(await dialog.evaluate((d) => d.scrollWidth - d.clientWidth)).toBeLessThanOrEqual(0);
  await dialog.getByRole("button", { name: "Отмена" }).click();
  await noSideScroll(page);
});

test("phone: 3D - a swipe up scrolls the page, sideways rotates", async ({ page }) => {
  await open(page, "Рисунки/Интерактив");
  const canvas = page.locator(".k-plot-canvas");
  await canvas.scrollIntoViewIfNeeded();
  await page.evaluate(async () => void (await document.fonts.ready));
  const frame = () => canvas.evaluate((c: HTMLCanvasElement) => c.toDataURL());
  let first = "";
  await expect.poll(async () => first === (first = await frame())).toBe(true);
  const cdp = await page.context().newCDPSession(page);
  // From the middle of the figure where it is now (scrolling moves it).
  const swipe = async (dx: number, dy: number) => {
    const box = (await canvas.boundingBox())!;
    const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
    const point = (k: number) => [{ x: x + dx * k, y: y + dy * k }];
    await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: point(0) });
    for (let k = 1; k <= 8; k++) await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: point(k / 8) });
    await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  };

  const top = await page.evaluate(() => document.getElementById("page")!.scrollTop);
  await swipe(0, -120);
  await expect.poll(() => page.evaluate(() => document.getElementById("page")!.scrollTop)).toBeGreaterThan(top + 40);
  expect(await frame()).toBe(first);

  await canvas.scrollIntoViewIfNeeded();
  await swipe(120, 0);
  await expect.poll(frame).not.toBe(first);
});
