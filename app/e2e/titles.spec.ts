// Titles instead of file names: a note is named by `title:` in its file, a
// folder by `_folder.toml`. The tree, the header, a tab and links without
// their own caption show titles; a file edit shows without a reload.
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { VAULT, open } from "./helpers";

const ODD = String.raw`@#$@&$*%@#!.:/\ — в названии можно всё`;

test("the tree, the header, a tab and links use the titles from the files", async ({ page }) => {
  await open(page, "Имена/странное");
  const tree = page.locator("#tree");
  // The folder "Имена" by its title from `_folder.toml`, the note from its file.
  await expect(tree.locator("summary .tree-name", { hasText: "Имена: файлы / названия" })).toBeVisible();
  await expect(tree.locator('a[data-id="Имена/странное"]')).toHaveText(ODD);
  await expect(page.locator("#crumbs")).toHaveText(`Имена: файлы / названия / ${ODD}`);
  await expect(page).toHaveTitle(`${ODD} — Заметки`);
  // The tab bar from the second tab on.
  await page.keyboard.press("Alt+KeyT");
  await expect(page.locator(".tabbar .tab").first()).toContainText(ODD);
  await page.keyboard.press("Alt+KeyW");
  // An error in `_folder.toml`: the folder by its name.
  await expect(tree.locator("summary .tree-name", { hasText: /^б$/ })).toHaveCount(1);

  // A link without a caption: the title of the target.
  await open(page, "Особые случаи/Ссылки");
  await expect(page.locator('#note a.k-link[data-k-target="Имена/странное"]')).toHaveText(ODD);
});

test("an edit of the title and _folder.toml shows at once", async ({ page }) => {
  const dir = join(VAULT, "Переименование");
  const note = (title: string) => `#import "/_baluk/lib.typ": *\n#show: note.with(title: [${title}])\n\nТекст.\n`;
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "a.typ"), note("Первое название"));
  try {
    await open(page, "Переименование/a");
    const tree = page.locator("#tree");
    await expect(tree.locator('a[data-id="Переименование/a"]')).toHaveText("Первое название");
    await expect(tree.locator("summary .tree-name", { hasText: "Переименование" })).toBeVisible();

    writeFileSync(join(dir, "_folder.toml"), 'title = "Папка: новое имя?"\n');
    await expect(tree.locator("summary .tree-name", { hasText: "Папка: новое имя?" })).toBeVisible();

    writeFileSync(join(dir, "a.typ"), note("Второе название"));
    await expect(tree.locator('a[data-id="Переименование/a"]')).toHaveText("Второе название");
    await expect(page.locator("#crumbs")).toHaveText("Папка: новое имя? / Второе название");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a formula in a title as source text; in \"Ссылаются сюда\" the heading with the formula", async ({ page }) => {
  const dir = join(VAULT, "Формулы в названиях");
  mkdirSync(dir, { recursive: true });
  const head = (title: string) => `#import "/_baluk/lib.typ": *\n#show: note.with(title: [${title}])\n\n`;
  writeFileSync(join(dir, "ряд.typ"), `${head("Ряд $sum 1/n^2$")}См. #see("Формулы и теги", anchor: "Пространство-ℝ𝑛-и-норма-‖𝑥‖2").\n`);
  try {
    await open(page, "Формулы в названиях/ряд");
    await expect(page.locator('#tree a[data-id="Формулы в названиях/ряд"]')).toHaveText("Ряд sum 1/n^2");
    await expect(page.locator("#crumbs b")).toHaveText("Ряд sum 1/n^2");

    await open(page, "Формулы и теги");
    const item = page.locator("#backlinks li", { hasText: "Ряд sum 1/n^2" });
    await expect(item.locator(".backlinks-heading math")).toHaveCount(2);
    await expect(item.locator(".backlinks-heading strong")).toHaveText("норма");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
