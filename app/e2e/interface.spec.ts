import { rmSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { VAULT, open, ready, resetTheme, title, vaultUrl } from "./helpers";

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

test("тема — с первого кадра: запомненная, ещё до настроек с сервера", async ({ page }) => {
  await open(page, "Сеть/SSH");
  try {
    await page.locator("#theme").click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "night");
    // Настройки не пришли (клиент не запустится), а тема и фон — уже её.
    await page.route("**/api/vaults/*/settings", (r) => r.abort());
    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "night");
    expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).not.toBe("rgb(255, 255, 255)");
  } finally {
    await page.unrouteAll();
    await resetTheme(page);
  }
});

test("переход к собранной заметке — без пустого кадра между заметками", async ({ page }) => {
  await open(page, "Сеть/UFW");
  await open(page, "Сеть/SSH");
  await page.evaluate(() => {
    const note = document.getElementById("note")!;
    new MutationObserver(() => {
      if (!note.querySelector(".note-body")) document.documentElement.dataset.blank = "1";
    }).observe(note, { childList: true });
  });
  await page.locator("#tree").getByRole("link", { name: "UFW" }).click();
  await expect(title(page)).toHaveText("UFW");
  await expect(page.locator("html")).not.toHaveAttribute("data-blank", "1");
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

test("раскрытые «Ответы» — по разделу и тексту: блок выше не раскрывает чужой", async ({ page }) => {
  const dir = join(VAULT, "Ответы");
  const file = join(dir, "Заметка.typ");
  const text = (extra: boolean) =>
    `#import "/_baluk/lib.typ": *\n#show: note.with(title: [Ответы])\n\n= Первый\n\n${extra ? "#quiz(([Новый вопрос], [Добавлены выше.]))\n\n" : ""}` +
    `= Второй\n\n#quiz(([Вопрос], [Раскрытые.]))\n`;
  try {
    mkdirSync(dir, { recursive: true });
    writeFileSync(file, text(false));
    await page.goto(vaultUrl("/"));
    await ready(page);
    await page.locator("#refresh").click();
    await page.locator("#tree").locator('a[data-id="Ответы/Заметка"]').click();
    await ready(page);
    const answers = page.locator("#note details", { hasText: "Раскрытые." });
    await answers.locator("summary").click();
    await expect(answers).toHaveAttribute("open", "");
    writeFileSync(file, text(true));
    await page.locator("#refresh").click();
    await expect(page.locator("#note details")).toHaveCount(2);
    await expect(answers).toHaveAttribute("open", "");
    await expect(page.locator("#note details", { hasText: "Добавлены выше." })).not.toHaveAttribute("open", "");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("настройки: кегль — для всех хранилищ, по выбору — только для этого; прочее — для хранилища", async ({ page }) => {
  await open(page, "Сеть/SSH");
  await page.locator("#open-settings").click();
  const size = page.locator('input[id="setting-appearance.font_size"]');
  const row = page.locator('.setting[data-key="appearance.font_size"]');
  const shared = async () => (await (await page.request.get("/api/settings")).json()).values["appearance.font_size"];
  await expect(row).toHaveAttribute("data-source", "default");
  await size.fill("23");
  await size.dispatchEvent("change");
  await expect(page.locator("html")).toHaveCSS("--k-size", "23px");
  await expect(row.locator(".setting-badge")).toHaveText("изменено для всех хранилищ");
  expect(await shared()).toBe(23);
  await page.reload();
  await ready(page);
  await expect(page.locator("html")).toHaveCSS("--k-size", "23px");

  // Только для этого хранилища: дальше кегль меняется лишь здесь.
  await page.locator("#open-settings").click();
  await row.getByRole("button", { name: "только для этого хранилища" }).click();
  await expect(row.locator(".setting-badge")).toHaveText("только в этом хранилище");
  await size.fill("25");
  await size.dispatchEvent("change");
  await expect(page.locator("html")).toHaveCSS("--k-size", "25px");
  expect(await shared()).toBe(23);
  await row.getByRole("button", { name: "как у всех" }).click();
  await expect(page.locator("html")).toHaveCSS("--k-size", "23px");
  await row.getByRole("button", { name: "по умолчанию" }).click();
  await expect(row).toHaveAttribute("data-source", "default");
  await expect(page.locator("html")).toHaveCSS("--k-size", "19px");

  // Прочие настройки — только для хранилища.
  const tags = page.locator('.setting[data-key="header.tags"]');
  await tags.locator("input").uncheck();
  await expect(tags.locator(".setting-badge")).toHaveText("только в этом хранилище");
  await tags.getByRole("button", { name: "как у всех" }).click();
  await expect(tags).toHaveAttribute("data-source", "default");
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

test("оглавление: заголовок с формулой — формулой, а не текстом", async ({ page }) => {
  await page.setViewportSize({ width: 1700, height: 900 });
  await open(page, "Формулы и теги");
  const item = page.locator(".toc a", { hasText: "Пространство" });
  await expect(item.locator("math")).toHaveCount(2);
  await expect(item.locator("strong")).toHaveText("норма");
  await expect(item.locator(".k-num")).toHaveCount(0);
  await item.click();
  await expect(page.locator("#note h2", { hasText: "Пространство" })).toBeInViewport();
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
    await page.goto(vaultUrl("/"));
    await ready(page);
    await page.locator("#refresh").click();
    await page.locator("#tree").locator('a[data-id="Правка/Заметка"]').click();
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

test("правка файла приходит событием сервера — без кнопки; «только по кнопке» — нет", async ({ page }) => {
  const dir = join(VAULT, "События");
  const file = join(dir, "Заметка.typ");
  const text = (n: number) => `#import "/_baluk/lib.typ": *\n#show: note.with(title: [События])\n\nВерсия ${n}.\n`;
  // Сервер следит за файлами: опроса нет, обновляет событие.
  const mode = async (value: "auto" | "manual") =>
    expect((await page.request.put("/api/settings", { data: { "refresh.mode": value } })).ok()).toBe(true);
  try {
    await mode("auto");
    const events = page.waitForResponse((r) => r.url().includes("/events"));
    await page.goto(vaultUrl("/"));
    await ready(page);
    await events;
    mkdirSync(dir, { recursive: true });
    writeFileSync(file, text(1));
    // Новая заметка — в дереве, правка — на экране, без ⟳.
    const link = page.locator("#tree").locator('a[data-id="События/Заметка"]');
    await expect(link).toBeVisible();
    await link.click();
    await ready(page);
    await expect(page.locator("#note")).toContainText("Версия 1.");
    writeFileSync(file, text(2));
    await expect(page.locator("#note")).toContainText("Версия 2.");

    // «Только по кнопке»: событие пришло, а заметка прежняя — до «Обновить».
    await mode("manual");
    await page.reload();
    await ready(page);
    writeFileSync(file, text(3));
    // В автоматическом режиме правка приходит за доли секунды.
    await page.waitForTimeout(1500);
    await expect(page.locator("#note")).toContainText("Версия 2.");
    await page.locator("#refresh").click();
    await expect(page.locator("#note")).toContainText("Версия 3.");
  } finally {
    rmSync(dir, { recursive: true, force: true });
    await mode("auto");
  }
});

test("нет связи с сервером: заметка остаётся, метка в строке; связь вернулась — метка пропадает", async ({ page }) => {
  await open(page, "Сеть/SSH");
  const offline = page.locator("#offline");
  await expect(offline).toHaveCount(0);
  // Сервер «выключили»: запросы не доходят.
  await page.route("**/api/**", (route) => route.abort("connectionrefused"));
  await page.locator("#refresh").click();
  await expect(offline).toBeVisible();
  await expect(offline).toHaveText("нет связи");
  await expect(offline).toHaveAttribute("title", /Нет связи с сервером/);
  await expect(title(page)).toHaveText("SSH");
  // Другая заметка без связи — объяснение, а не ошибка запроса.
  await page.locator("#tree").getByRole("link", { name: "UFW" }).click();
  await expect(page.locator("#note")).toContainText("заметка откроется, когда связь вернётся");
  // Связь вернулась: пробный запрос (нажатие на метку) — метка пропала, заметка догрузилась.
  await page.unroute("**/api/**");
  await offline.click();
  await expect(offline).toHaveCount(0);
  await expect(title(page)).toHaveText("UFW");
});

test("PDF заметки в текущей теме", async ({ page }) => {
  await open(page, "Сеть/SSH");
  const [popup] = await Promise.all([page.waitForEvent("popup"), page.locator("#pdf").click()]);
  expect(decodeURIComponent(popup.url())).toContain("/pdf/Сеть/SSH?theme=classic");
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
