import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { VAULT, apiPath, noteUrl, open, ready, title, vaultUrl } from "./helpers";

test("home: the graph and the list, a click on a node opens the note", async ({ page }) => {
  await page.goto(vaultUrl("/"));
  await ready(page);
  await expect(page.locator(".home-lead")).toContainText("2 книги");
  await expect(page.locator(".graph-node")).not.toHaveCount(0);
  // By the circle: the node center (circle + label) may fall into the gap between them.
  await page.locator('.graph-node[data-id="Сеть/SSH"] circle').click();
  await ready(page);
  await expect(title(page)).toHaveText("SSH");
  await expect(page).toHaveURL(noteUrl("Сеть/SSH"));
});

test("a link into a book chapter by text and by label; back and forward", async ({ page }) => {
  await open(page, "Особые случаи/Ссылки");
  await page.locator("#note").getByRole("link", { name: "Код из файла" }).click();
  await ready(page);
  await expect(page).toHaveURL(noteUrl("Книга", "Код-из-файла"));
  await expect(page.locator("#note h3", { hasText: "Код из файла" })).toBeInViewport();
  await expect(page.locator(".chapter-nav")).toContainText("1. Основы");

  // By the label: <особый> is the same section of the book.
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

test("book chapters: next, previous, the outline leads to the right chapter", async ({ page }) => {
  await open(page, "Книга");
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Основы/);
  await page.locator(".chapter-nav a.next").click();
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Продолжение/);
  await expect(page.locator("#Итоги-2")).toBeAttached();
  await expect(page.locator("#Итоги")).not.toBeAttached();
  await page.locator(".chapter-nav a.prev").click();
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Основы/);
});

test("the book was edited: going to a chapter shows the new version, not the stock", async ({ page }) => {
  const file = join(VAULT, "Книга", "02-продолжение.typ");
  const before = readFileSync(file, "utf8");
  // "Только по кнопке": the edit event does not reload the note by itself.
  const mode = async (value: "auto" | "manual") =>
    expect((await page.request.put("/api/settings", { data: { "refresh.mode": value } })).ok()).toBe(true);
  try {
    await mode("manual");
    const prefetched = page.waitForResponse((r) => decodeURIComponent(r.url()).endsWith("/notes/Книга?chapter=1"));
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

test("Ctrl+F in a book by chapters: search over the whole book with chapter numbers; a second one goes to the browser", async ({ page }) => {
  await open(page, "Книга");
  await page.keyboard.press("Control+KeyF");
  const palette = page.locator(".palette");
  await expect(palette.locator("input")).toHaveAttribute("placeholder", /книги «/);
  await palette.locator("input").fill("Итоги");
  // "Итоги" is in every chapter: they differ by the chapter number.
  const chapters = palette.locator(".p-chapter");
  for (const n of [1, 2, 3]) await expect(chapters.filter({ hasText: `гл. ${n}` }).first()).toBeVisible();
  await palette.locator("input").press("Control+KeyF");
  await expect(palette).toBeHidden();
  // Choosing a result goes to the chapter.
  await page.keyboard.press("Control+KeyF");
  await palette.locator("input").fill("Итоги");
  await expect(chapters.first()).toBeVisible();
  await palette.locator(".palette-list li[role=option]", { has: page.locator(".p-chapter", { hasText: "гл. 2" }) }).click();
  await ready(page);
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Продолжение/);
  await expect(page.locator("#Итоги-2")).toBeInViewport();
  // Where to search - the shown chapter: only its sections, without a chapter number.
  await page.keyboard.press("Control+KeyF");
  const scopes = palette.locator(".palette-scopes [role=radio]");
  await expect(scopes).toHaveText(["Глава", "Книга", "Всё хранилище"]);
  await scopes.first().click();
  await expect(palette.locator("input")).toHaveAttribute("placeholder", /главы «Продолжение»/);
  await palette.locator("input").fill("Итоги");
  await expect(palette.locator(".palette-group")).toContainText("В главе «Продолжение»");
  await expect(palette.locator(".palette-list li[role=option]")).toHaveCount(1);
  await expect(palette.locator(".palette-list li[role=option]")).toContainText("Итоги второй");
});

test("the server serves a book chapter: one chapter in the page, a reload gives the same chapter", async ({ page }) => {
  const requested: string[] = [];
  page.on("request", (r) => {
    const url = new URL(r.url());
    if (apiPath(url).startsWith("/api/notes/")) requested.push(decodeURIComponent(apiPath(url) + url.search));
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

  // The outline is of the whole book; an item from another chapter loads it.
  await page.locator(".toc a", { hasText: "Картинка из файла" }).first().dispatchEvent("click");
  await ready(page);
  await expect(page.locator("#Картинка-из-файла")).toBeInViewport();

  await page.reload();
  await ready(page);
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Приложение/);
  expect(requested.at(-1)).toBe("/api/notes/Книга?anchor=Картинка-из-файла");
});

test("neighbouring chapters in advance: navigation without a server request", async ({ page }) => {
  const requested: string[] = [];
  page.on("request", (r) => {
    const url = new URL(r.url());
    if (apiPath(url).startsWith("/api/notes/")) requested.push(decodeURIComponent(apiPath(url) + url.search));
  });
  const prefetched = page.waitForResponse((r) => decodeURIComponent(r.url()).endsWith("/notes/Книга?chapter=1"));
  await open(page, "Книга");
  // The response came whole and was parsed by the client (the stock fills after parsing).
  await (await prefetched).finished();
  await page.waitForTimeout(200);
  const before = requested.length;
  await page.keyboard.press("BracketRight");
  await ready(page);
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Продолжение/);
  expect(requested.slice(before)).not.toContain("/api/notes/Книга?chapter=1");
  // And back - from the stock too (the shown chapter stays a neighbour).
  await page.keyboard.press("BracketLeft");
  await ready(page);
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Основы/);
  expect(requested.slice(before)).not.toContain("/api/notes/Книга?chapter=0");
});

test("names with + # % and deep nesting", async ({ page }) => {
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

test("a broken link leads nowhere", async ({ page }) => {
  await open(page, "Особые случаи/Ссылки");
  const broken = page.locator('#note a.k-link[data-k-target="Нет/Такой заметки"]');
  await expect(broken).not.toHaveAttribute("href");
});

test("a build error and warnings", async ({ page }) => {
  await open(page, "Особые случаи/Ошибка компиляции");
  await expect(page.locator("#problems .errors")).toContainText("unknown variable: no-such-function");
  await open(page, "Особые случаи/Предупреждение");
  await expect(page.locator("#problems summary")).toHaveText("Предупреждения: 4");
});

test("a missing note", async ({ page }) => {
  await open(page, "Нет/Такой");
  await expect(page.locator("#note")).toContainText("Заметки «Нет/Такой» нет.");
});

test("backlinks", async ({ page }) => {
  await open(page, "Сеть/SSH");
  const bl = page.locator("#backlinks");
  await expect(bl).toContainText("UFW");
  await expect(bl).toContainText("Ссылки");
  await bl.getByRole("link", { name: "UFW" }).click();
  await ready(page);
  await expect(title(page)).toHaveText("UFW");
});

test("what the book shares is at the top of the outline in any chapter: the title and the root tags", async ({ page }) => {
  await page.setViewportSize({ width: 1700, height: 900 });
  await open(page, "Книга");
  await page.keyboard.press("BracketRight");
  await ready(page);
  const info = page.locator(".toc .book-info");
  await expect(info).toBeVisible();
  await expect(info.locator(".book-info-title")).toHaveText("Тестовая книга");
  await expect(info.locator(".book-info-tags a")).toHaveText(["#книга", "#фикстура"]);
  // Under a chapter heading - only its own tags.
  await expect(page.locator("#note .k-chapter-tags li")).toHaveText(["код"]);
  await info.locator(".book-info-title").click();
  await ready(page);
  await expect(page.locator("#note h2.k-h1")).toHaveText(/Основы/);
  // A note has none.
  await open(page, "демо/компоненты");
  await expect(page.locator(".toc")).toBeVisible();
  await expect(page.locator(".toc .book-info")).toHaveCount(0);
});
