// Названия вместо имён файлов: заметка называется по `title:` в своём
// файле, папка — по `_folder.toml`. Дерево, шапка, вкладка и ссылки без
// своей подписи показывают названия; правка файла видна без перезагрузки.
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { VAULT, open } from "./helpers";

const ODD = String.raw`@#$@&$*%@#!.:/\ — в названии можно всё`;

test("дерево, шапка, вкладка и ссылки — названиями из файлов", async ({ page }) => {
  await open(page, "Имена/странное");
  const tree = page.locator("#tree");
  // Папка «Имена» — названием из `_folder.toml`, заметка — из своего файла.
  await expect(tree.locator("summary .tree-name", { hasText: "Имена: файлы / названия" })).toBeVisible();
  await expect(tree.locator('a[data-id="Имена/странное"]')).toHaveText(ODD);
  await expect(page.locator("#crumbs")).toHaveText(`Имена: файлы / названия / ${ODD}`);
  await expect(page).toHaveTitle(`${ODD} — Заметки`);
  // Полоса вкладок — со второй вкладки.
  await page.keyboard.press("Alt+KeyT");
  await expect(page.locator(".tabbar .tab").first()).toContainText(ODD);
  await page.keyboard.press("Alt+KeyW");
  // Ошибка в `_folder.toml` — папка своим именем.
  await expect(tree.locator("summary .tree-name", { hasText: /^б$/ })).toHaveCount(1);

  // Ссылка без подписи — название цели.
  await open(page, "Особые случаи/Ссылки");
  await expect(page.locator('#note a.k-link[data-k-target="Имена/странное"]')).toHaveText(ODD);
});

test("правка названия и _folder.toml — видна сразу", async ({ page }) => {
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

test("формула в названии — текстом исходника; в «Ссылаются сюда» — заголовок с формулой", async ({ page }) => {
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
