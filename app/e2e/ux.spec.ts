import { expect, test, type Page } from "@playwright/test";
import { noteUrl, open, ready, title } from "./helpers";

const palette = (page: Page) => page.locator(".palette");

async function ask(page: Page, query: string) {
  await page.keyboard.press("Control+o");
  await expect(palette(page)).toBeVisible();
  await page.locator(".palette input").fill(query);
}

test("Ctrl+O: нечёткий переход к заметке", async ({ page }) => {
  await open(page, "Сеть/SSH");
  await ask(page, "дно");
  await expect(page.locator(".palette-list li.selected")).toContainText("Дно");
  await page.keyboard.press("Enter");
  await ready(page);
  await expect(title(page)).toHaveText("Дно");
  await expect(palette(page)).toBeHidden();
});

test("поиск по тексту ведёт в раздел нужной главы", async ({ page }) => {
  await open(page, "Сеть/SSH");
  await ask(page, "/итоги второй");
  const hit = page.locator(".palette-list li[role=option]").first();
  await expect(hit).toContainText("Тестовая книга › Итоги");
  await expect(hit.locator("mark").first()).toHaveText("Итоги");
  await page.keyboard.press("Enter");
  await ready(page);
  await expect(page).toHaveURL(noteUrl("Книга", "Итоги-2"));
  await expect(page.locator("#Итоги-2")).toBeInViewport();
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Продолжение/);
});

test("команды: сменить тему из палитры", async ({ page }) => {
  await open(page, "Сеть/SSH");
  await page.keyboard.press("Control+k");
  await page.locator(".palette input").pressSequentially("тема ночь");
  await page.keyboard.press("Enter");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "ночь");
  await page.keyboard.press("Control+k");
  await page.locator(".palette input").pressSequentially("как в системе");
  await page.keyboard.press("Enter");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "классика");
});

test("теги: из шапки заметки, страница тега, все теги, палитра", async ({ page }) => {
  await open(page, "Сеть/SSH");
  await page.locator("#note .k-tags a", { hasText: "сеть" }).click();
  await ready(page);
  await expect(page).toHaveURL(`/tags/${encodeURIComponent("сеть")}`);
  const list = page.locator(".tag-notes");
  await expect(list).toContainText("SSH");
  await expect(list).toContainText("UFW");
  await page.getByRole("link", { name: "← все теги" }).click();
  await expect(page.locator(".tag-cloud")).toContainText("#фикстура");

  await ask(page, "#фикс");
  await page.keyboard.press("Enter");
  await expect(page.locator("h1")).toHaveText("#фикстура");
});

test("превью ссылки при наведении", async ({ page }) => {
  await open(page, "Сеть/UFW");
  await page.locator("#note a.k-link", { hasText: "порт мог измениться" }).hover();
  const card = page.locator(".link-preview");
  await expect(card).toBeVisible();
  await expect(card.locator(".lp-title")).toContainText("SSH › Смена порта");
  await page.mouse.move(5, 5);
  await expect(card).toBeHidden();
});

test("вкладки: Ctrl+клик, переключение, закрытие", async ({ page }) => {
  await open(page, "Особые случаи/Ссылки");
  await page.locator("#note").getByRole("link", { name: "Дно", exact: true }).click({ modifiers: ["Control"] });
  await ready(page);
  const tabs = page.locator(".tabbar .tab");
  await expect(tabs).toHaveCount(2);
  await expect(tabs.nth(1)).toHaveClass(/active/);
  await expect(title(page)).toHaveText("Дно");

  await tabs.nth(0).click();
  await ready(page);
  await expect(title(page)).toHaveText("Ссылки");
  await tabs.nth(1).locator(".tab-close").click();
  await expect(page.locator(".tabbar")).toBeHidden();
  await expect(title(page)).toHaveText("Ссылки");
});

test("память места: назад и повторное открытие", async ({ page }) => {
  // Место — это текст вверху окна (прокрутка в пикселях может сдвинуться,
  // если выше дорисовался рисунок: браузер держит видимый текст на месте).
  const topText = () =>
    page.evaluate(() => {
      // Первый блок от y = 160 вниз: точка может прийтись на промежуток между
      // блоками (высота строк зависит от шрифтов системы).
      for (let y = 160; y < innerHeight; y += 8) {
        const el = document.elementFromPoint(innerWidth / 2 + 130, y);
        const block = el?.closest("p, li, h2, h3, h4, figure, pre, table, div.k-box");
        if (block) return block.textContent?.slice(0, 60) ?? "";
      }
      return "";
    });
  await open(page, "демо/компоненты");
  await page.evaluate(() => scrollTo(0, 1500));
  await page.waitForTimeout(400); // место запоминается по ходу прокрутки
  const before = await topText();
  expect(before).not.toBe("");

  await page.locator("#tree").getByRole("link", { name: "SSH" }).click();
  await ready(page);
  await page.goBack();
  await ready(page);
  await expect.poll(topText).toBe(before);

  await page.locator("#tree").getByRole("link", { name: "SSH" }).click();
  await ready(page);
  await page.locator("#tree").getByRole("link", { name: "компоненты" }).click();
  await ready(page);
  await expect.poll(topText).toBe(before);
});

test("клавиши: главы, справка, режим чтения", async ({ page }) => {
  await open(page, "Книга");
  await page.keyboard.press("]");
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Продолжение/);
  await page.keyboard.press("[");
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Основы/);

  await page.keyboard.press("?");
  const help = page.getByRole("dialog", { name: "Горячие клавиши" });
  await expect(help).toBeVisible();
  await expect(help).toContainText("Быстрый переход к заметке");
  await page.keyboard.press("Escape");
  await expect(help).toBeHidden();

  await page.keyboard.press("f");
  await expect(page.locator("#sidebar")).toBeHidden();
  await page.keyboard.press("Escape");
  await expect(page.locator("#sidebar")).toBeVisible();
});

test("дерево: свёрнутая папка запоминается, путь к открытой заметке раскрывается", async ({ page }) => {
  await open(page, "Сеть/SSH");
  const folder = page.locator("#tree details", { has: page.locator("summary", { hasText: /^Глубоко$/ }) });
  await folder.locator("> summary").click();
  await expect(folder).not.toHaveAttribute("open");
  await page.reload();
  await ready(page);
  await expect(folder).not.toHaveAttribute("open");
  await ask(page, "дно");
  await page.keyboard.press("Enter");
  await ready(page);
  await expect(folder).toHaveAttribute("open");
  await expect(page.locator('#tree a[data-id="Глубоко/а/б/в/г/Дно"]')).toBeVisible();
});
