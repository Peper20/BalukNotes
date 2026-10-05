import { rmSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { VAULT, open, ready, resetTheme, title, vaultUrl } from "./helpers";

test("theme: every click gives a different look at once (\"как в системе\" is in the settings)", async ({ page }) => {
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

test("theme from the first frame: the remembered one, before the settings from the server", async ({ page }) => {
  await open(page, "Сеть/SSH");
  try {
    await page.locator("#theme").click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "night");
    // The settings did not come (the client will not start), but the theme and the background are already its own.
    await page.route("**/api/vaults/*/settings", (r) => r.abort());
    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "night");
    expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).not.toBe("rgb(255, 255, 255)");
  } finally {
    await page.unrouteAll();
    await resetTheme(page);
  }
});

test("going to a built note: no empty frame between notes", async ({ page }) => {
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

test("open \"Ответы\" do not collapse when the note is rebuilt", async ({ page }) => {
  await open(page, "демо/компоненты");
  const answers = page.locator(".k-quiz-answers").first();
  await answers.locator("summary").click();
  await expect(answers).toHaveAttribute("open", "");
  // A mark on the old markup: gone means the HTML was inserted anew.
  await page.locator("#note .note-body > *").first().evaluate((el) => el.setAttribute("data-old", ""));
  await page.locator("#refresh").click();
  await expect(page.locator("#note [data-old]")).toHaveCount(0);
  await expect(answers).toHaveAttribute("open", "");
});

test("open \"Ответы\" by section and text: a block above does not open someone else's", async ({ page }) => {
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

test("settings: the font size for all vaults, optionally only for this one; the rest for the vault", async ({ page }) => {
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

  // Only for this vault: from now on the font size changes only here.
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

  // Other settings: only for the vault.
  const tags = page.locator('.setting[data-key="header.tags"]');
  await tags.locator("input").uncheck();
  await expect(tags.locator(".setting-badge")).toHaveText("только в этом хранилище");
  await tags.getByRole("button", { name: "как у всех" }).click();
  await expect(tags).toHaveAttribute("data-source", "default");
});

test("outline: beside on a wide screen, a popup on a narrow one", async ({ page }) => {
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

test("with tabs: a heading by anchor and the outline are under the panel, not behind it", async ({ page }) => {
  await page.setViewportSize({ width: 1700, height: 900 });
  await open(page, "демо/компоненты");
  await page.keyboard.press("Alt+KeyT");
  await ready(page);
  await open(page, "демо/компоненты");
  await expect(page.locator(".tabbar .tab")).toHaveCount(2);
  const panel = (await page.locator(".chrome").boundingBox())!;
  const toc = page.locator(".toc");
  expect((await toc.boundingBox())!.y).toBeGreaterThanOrEqual(panel.y + panel.height);
  const item = toc.locator("a").nth(2);
  const target = decodeURIComponent((await item.getAttribute("href"))!.slice(1));
  await item.click();
  const heading = page.locator(`[id="${target}"]`);
  await expect(async () => {
    const y = (await heading.boundingBox())!.y;
    expect(y).toBeGreaterThanOrEqual(panel.y + panel.height);
    expect(y).toBeLessThan(panel.y + panel.height + 40);
    expect(decodeURIComponent((await toc.locator("a.current").getAttribute("href"))!.slice(1))).toBe(target);
  }).toPass();
});

test("outline: a heading with a formula as a formula, not text", async ({ page }) => {
  await page.setViewportSize({ width: 1700, height: 900 });
  await open(page, "Формулы и теги");
  const item = page.locator(".toc a", { hasText: "Пространство" });
  await expect(item.locator("math")).toHaveCount(2);
  await expect(item.locator("strong")).toHaveText("норма");
  await expect(item.locator(".k-num")).toHaveCount(0);
  await item.click();
  await expect(page.locator("#note h2", { hasText: "Пространство" })).toBeInViewport();
});

test("a new note shows up in the tree without a reload", async ({ page }) => {
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

test("a file change is picked up by ⟳ without losing the place", async ({ page }) => {
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
    await page.mouse.move(700, 400); // over the note, not over the tree (it scrolls by itself)
    await page.mouse.wheel(0, 800);
    await expect.poll(() => page.evaluate(() => document.getElementById("page")!.scrollTop)).toBeGreaterThan(300);
    const y = await page.evaluate(() => document.getElementById("page")!.scrollTop);
    writeFileSync(file, text(2));
    await page.locator("#refresh").click();
    await expect(page.locator("#note")).toContainText("Версия 2.");
    expect(Math.abs((await page.evaluate(() => document.getElementById("page")!.scrollTop)) - y)).toBeLessThan(5);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a file edit comes as a server event without the button; \"только по кнопке\" - it does not", async ({ page }) => {
  const dir = join(VAULT, "События");
  const file = join(dir, "Заметка.typ");
  const text = (n: number) => `#import "/_baluk/lib.typ": *\n#show: note.with(title: [События])\n\nВерсия ${n}.\n`;
  // The server watches the files: no polling, an event updates.
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
    // The new note is in the tree, the edit on screen, without ⟳.
    const link = page.locator("#tree").locator('a[data-id="События/Заметка"]');
    await expect(link).toBeVisible();
    await link.click();
    await ready(page);
    await expect(page.locator("#note")).toContainText("Версия 1.");
    writeFileSync(file, text(2));
    await expect(page.locator("#note")).toContainText("Версия 2.");

    // "Только по кнопке": the event came, but the note is the old one until "Обновить".
    await mode("manual");
    await page.reload();
    await ready(page);
    writeFileSync(file, text(3));
    // In the automatic mode an edit arrives in a fraction of a second.
    await page.waitForTimeout(1500);
    await expect(page.locator("#note")).toContainText("Версия 2.");
    await page.locator("#refresh").click();
    await expect(page.locator("#note")).toContainText("Версия 3.");
  } finally {
    rmSync(dir, { recursive: true, force: true });
    await mode("auto");
  }
});

test("no connection to the server: the note stays, a label in the bar; the connection is back - the label goes", async ({ page }) => {
  await open(page, "Сеть/SSH");
  const offline = page.locator("#offline");
  await expect(offline).toHaveCount(0);
  // The server was "switched off": requests do not get through.
  await page.route("**/api/**", (route) => route.abort("connectionrefused"));
  await page.locator("#refresh").click();
  await expect(offline).toBeVisible();
  await expect(offline).toHaveText("нет связи");
  await expect(offline).toHaveAttribute("title", /Нет связи с сервером/);
  await expect(title(page)).toHaveText("SSH");
  // Another note without the connection: an explanation, not a request error.
  await page.locator("#tree").getByRole("link", { name: "UFW" }).click();
  await expect(page.locator("#note")).toContainText("заметка откроется, когда связь вернётся");
  // The connection is back: a probe request (clicking the label) - the label is gone, the note loaded.
  await page.unroute("**/api/**");
  await offline.click();
  await expect(offline).toHaveCount(0);
  await expect(title(page)).toHaveText("UFW");
});

test("note PDF in the current theme", async ({ page }) => {
  await open(page, "Сеть/SSH");
  const [popup] = await Promise.all([page.waitForEvent("popup"), page.locator("#pdf").click()]);
  expect(decodeURIComponent(popup.url())).toContain("/pdf/Сеть/SSH?theme=classic");
  await popup.close();
});

test("phone: the sidebar slides in and hides after choosing a note", async ({ page }) => {
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

test("settings: extra packages come with a warning; a wrong entry is an error, a right one is saved", async ({ page }) => {
  try {
    await open(page, "Сеть/SSH");
    await page.locator("#open-settings").click();
    const row = page.locator('.setting[data-key="device.packages"]');
    const input = row.locator('input[type="text"]');
    await expect(row.locator(".setting-warning")).toContainText("чужой код");
    await input.fill("fletcher");
    await input.dispatchEvent("change");
    await expect(page.locator(".settings-error")).toContainText("@namespace/name:version");
    await input.fill("@preview/fletcher:0.5.8,  @preview/tablem:0.2.0");
    await input.dispatchEvent("change");
    await expect(row.locator(".setting-badge")).toHaveText("изменено на этом устройстве");
    const saved = async () => (await (await page.request.get("/api/settings")).json()).values["device.packages"];
    expect(await saved()).toBe("@preview/fletcher:0.5.8 @preview/tablem:0.2.0");
  } finally {
    await page.request.put("/api/settings", { data: { "device.packages": "" } });
  }
});
