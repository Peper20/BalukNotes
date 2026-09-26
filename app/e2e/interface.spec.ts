import { rmSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { VAULT, open, ready, resetTheme, title } from "./helpers";

test("тема: каждый клик — сразу другая на вид («как в системе» — в настройках)", async ({ page }) => {
  await open(page, "Сеть/SSH");
  const html = page.locator("html");
  await expect(html).toHaveAttribute("data-theme", "classic");
  try {
    await page.locator("#theme").click();
    await expect(html).toHaveAttribute("data-theme", "night");
    await expect(page.locator("#theme")).toHaveAttribute("title", /Ночь.*«Классика»/);
    await page.locator("#theme").click();
    await expect(html).toHaveAttribute("data-theme", "classic");
  } finally {
    await resetTheme(page);
  }
});

test("раскрытые «Ответы» не сворачиваются, когда заметка пересобрана", async ({ page }) => {
  await open(page, "демо/компоненты");
  const answers = page.locator(".k-quiz-answers").first();
  await answers.locator("summary").click();
  await expect(answers).toHaveAttribute("open", "");
  // Метка на старой разметке: пропала — значит, HTML вставлен заново.
  await page.locator("#note .note-body > *").first().evaluate((el) => el.setAttribute("data-old", ""));
  await page.locator("#refresh").click();
  await expect(page.locator("#note [data-old]")).toHaveCount(0);
  await expect(answers).toHaveAttribute("open", "");
});

test("настройки: кегль меняется сразу и сохраняется", async ({ page }) => {
  await open(page, "Сеть/SSH");
  await page.locator("#open-settings").click();
  const size = page.locator('input[id="setting-appearance.font_size"]');
  await size.fill("23");
  await size.dispatchEvent("change");
  await expect(page.locator("html")).toHaveCSS("--k-size", "23px");
  await page.reload();
  await ready(page);
  await expect(page.locator("html")).toHaveCSS("--k-size", "23px");
  await page.locator("#open-settings").click();
  await size.fill("19");
  await size.dispatchEvent("change");
  await expect(page.locator("html")).toHaveCSS("--k-size", "19px");
});

test("оглавление: сбоку на широком экране, всплывающее на узком", async ({ page }) => {
  await page.setViewportSize({ width: 1700, height: 900 });
  await open(page, "демо/компоненты");
  const toc = page.locator(".toc");
  await expect(toc).toBeVisible();
  await expect(toc).not.toHaveClass(/open/);
  await expect(toc.locator("a.current")).toHaveCount(1);

  await page.setViewportSize({ width: 420, height: 800 });
  await expect(toc).toBeHidden();
  await page.locator("#toggle-toc").click();
  await expect(toc).toBeVisible();
  const last = toc.locator("a").last();
  const target = decodeURIComponent((await last.getAttribute("href"))!.slice(1));
  await last.click();
  await expect(toc).toBeHidden();
  await expect(page.locator(`[id="${target}"]`)).toBeInViewport();
});

test("новая заметка появляется в дереве без перезагрузки", async ({ page }) => {
  const dir = join(VAULT, "Новое");
  try {
    await open(page, "Сеть/SSH");
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "Живая.typ"), '#import "/_baluk/lib.typ": *\n#show: note.with(title: [Живая])\n\nПоявилась.\n');
    await page.locator("#refresh").click();
    const link = page.locator("#tree").getByRole("link", { name: "Живая" });
    await expect(link).toBeVisible();
    await link.click();
    await ready(page);
    await expect(title(page)).toHaveText("Живая");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("изменение файла подхватывается по ⟳ без потери места", async ({ page }) => {
  const dir = join(VAULT, "Правка");
  const file = join(dir, "Заметка.typ");
  const text = (n: number) =>
    `#import "/_baluk/lib.typ": *\n#show: note.with(title: [Правка])\n\n${"Абзац.\n\n".repeat(80)}Версия ${n}.\n`;
  try {
    mkdirSync(dir, { recursive: true });
    writeFileSync(file, text(1));
    await page.goto("/");
    await ready(page);
    await page.locator("#refresh").click();
    await page.locator("#tree").getByRole("link", { name: "Заметка" }).click();
    await ready(page);
    await page.mouse.move(700, 400); // над заметкой, а не над деревом (оно прокручивается само)
    await page.mouse.wheel(0, 800);
    await expect.poll(() => page.evaluate(() => scrollY)).toBeGreaterThan(300);
    const y = await page.evaluate(() => scrollY);
    writeFileSync(file, text(2));
    await page.locator("#refresh").click();
    await expect(page.locator("#note")).toContainText("Версия 2.");
    expect(Math.abs((await page.evaluate(() => scrollY)) - y)).toBeLessThan(5);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("PDF заметки в текущей теме", async ({ page }) => {
  await open(page, "Сеть/SSH");
  const [popup] = await Promise.all([page.waitForEvent("popup"), page.locator("#pdf").click()]);
  expect(decodeURIComponent(popup.url())).toContain("/api/pdf/Сеть/SSH?theme=classic");
  await popup.close();
});

test("телефон: панель выезжает и прячется после выбора заметки", async ({ page }) => {
  await page.setViewportSize({ width: 400, height: 800 });
  await open(page, "Сеть/SSH");
  const sidebar = page.locator("#sidebar");
  await expect(sidebar).not.toBeInViewport();
  await page.getByRole("button", { name: "Список заметок" }).click();
  await expect(sidebar).toBeInViewport();
  await sidebar.getByRole("link", { name: "UFW" }).click();
  await ready(page);
  await expect(title(page)).toHaveText("UFW");
  await expect(sidebar).not.toBeInViewport();
});
