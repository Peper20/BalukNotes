// DOM clobbering: a note whose images are named as parts of the document the
// client reads (`<img name="body">`, `name="getElementById"`...;
// tests/vault/Особые случаи/Имена элементов.typ). A named element shadows
// `document.<name>` for the whole page, so the client must not read those
// members by name: the note opens, its contents, search and tabs work, and the
// page logs no error.
import { expect, test } from "./fixtures";
import { noteUrl, open, ready, title } from "./helpers";

const ID = "Особые случаи/Имена элементов";
const TITLE = "Имена элементов";
const SECTIONS = ["location", "cookie", "forms", "body", "localStorage", "fetch"];

test("a note named like parts of the document: contents, links, search and tabs work; no page error", async ({ page }) => {
  await page.setViewportSize({ width: 1700, height: 900 });
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));

  await open(page, ID);
  await expect(title(page)).toHaveText(TITLE);
  // The note really shadows the document members here, so the scenario tests what it should.
  expect(await page.evaluate(() => typeof document.getElementById)).toBe("object");
  await expect(page.locator('#note img[name="body"]')).toHaveCount(1);
  await expect(page).toHaveTitle(`${TITLE} — Заметки`);

  // Contents: a click goes to the section and marks it as the current one.
  const toc = page.locator(".toc");
  await expect(toc).toBeVisible();
  for (const id of SECTIONS) await expect(toc.locator(`a[data-id="${id}"]`)).toHaveCount(1);
  await toc.locator('a[data-id="cookie"]').click();
  await expect(page).toHaveURL(noteUrl(ID, "cookie"));
  await expect(toc.locator("a.current")).toHaveAttribute("data-id", "cookie");
  await expect(page.locator('#note [id="cookie"]')).toBeInViewport();

  // A link to another note and back.
  await page.locator("#note").getByRole("link", { name: "SSH" }).click();
  await ready(page);
  await expect(title(page)).toHaveText("SSH");
  await page.goBack();
  await ready(page);
  await expect(title(page)).toHaveText(TITLE);

  // Search (Ctrl+K, the full-text mode "/"): the hit opens the note.
  await page.keyboard.press("Control+k");
  const palette = page.locator(".palette");
  await expect(palette).toBeVisible();
  await palette.locator("input").fill("/проверочная фраза");
  await expect(palette.locator(".palette-list li[role=option]", { hasText: TITLE })).toBeVisible();
  await page.keyboard.press("Enter");
  await ready(page);
  await expect(palette).toBeHidden();
  await expect(title(page)).toHaveText(TITLE);

  // Tabs: a new one, then back to the note's tab.
  await page.keyboard.press("Alt+KeyT");
  await ready(page);
  await expect(page.locator(".tabbar .tab")).toHaveCount(2);
  await page.locator(".tabbar .tab-title").first().click();
  await ready(page);
  await expect(title(page)).toHaveText(TITLE);
  await expect(page.locator(".tabbar .tab.active")).toContainText(TITLE);

  expect(errors).toEqual([]);
});
