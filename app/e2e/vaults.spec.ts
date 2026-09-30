// Хранилища (создать, переключиться — у каждого свои заметки) и удаление
// заметок и папок из интерфейса: в корзину (у сервера e2e — каталог TRASH), с
// подтверждением; вкладки удалённой заметки закрываются.
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { TRASH, VAULT, noteUrl, open, ready, vaultUrl } from "./helpers";

const head = (title: string) => `#import "/_baluk/lib.typ": *\n#show: note.with(title: [${title}])\n\n`;

test("хранилища: выбор без адреса, новое пустое, переключение — у каждого свои заметки", async ({ page }) => {
  const study = join(VAULT, "..", "Учёба");
  const work = join(VAULT, "..", "Работа");
  try {
    // Хранилища по умолчанию нет: новый браузер без адреса — экран выбора.
    await page.goto("/");
    await ready(page);
    const picker = page.locator("#vault-picker");
    await expect(picker.getByRole("link", { name: "vault" })).toBeVisible();
    // Неверное имя — ошибка сервера, ничего не создано.
    await picker.getByLabel("Новое хранилище").fill("a/b");
    await picker.getByRole("button", { name: "Создать" }).click();
    await expect(picker.getByRole("alert")).toContainText("недопустимое имя хранилища");
    await picker.getByLabel("Новое хранилище").fill("Учёба");
    await picker.getByLabel("Новое хранилище").press("Enter");

    // Новое хранилище — пустое, со своим адресом.
    await expect(page).toHaveURL(`/v/${encodeURIComponent("Учёба")}/`);
    await ready(page);
    await expect(page.locator("#vault-name")).toHaveText("Учёба");
    await expect(page.locator("#note h1")).toHaveText("Учёба");
    await expect(page.locator("#tree a")).toHaveCount(0);
    expect(existsSync(study)).toBe(true);

    // Меню: переход в другое хранилище — заметки прежние.
    await page.locator("#vault-switch").click();
    const menu = page.locator("#vault-menu");
    await expect(menu.getByRole("menuitem", { name: "Учёба", exact: true })).toHaveAttribute("aria-current", "true");
    await menu.getByRole("menuitem", { name: "vault", exact: true }).click();
    await expect(page).toHaveURL(vaultUrl("/"));
    await ready(page);
    await expect(page.locator("#tree").getByRole("link", { name: "SSH" })).toBeVisible();

    // Меню: «Новое хранилище…» — диалог.
    await page.locator("#vault-switch").click();
    await page.locator("#vault-menu").getByRole("menuitem", { name: "Новое хранилище…" }).click();
    const dialog = page.locator("#vault-new");
    await dialog.getByLabel("Название").fill("Работа");
    await dialog.getByRole("button", { name: "Создать" }).click();
    await expect(page).toHaveURL(`/v/${encodeURIComponent("Работа")}/`);
    await ready(page);
    expect(existsSync(work)).toBe(true);

    // Адрес без хранилища — в открытое последним.
    await page.goto(noteUrl("Сеть/SSH"));
    await ready(page);
    await page.goto("/n/Сеть/SSH");
    await ready(page);
    await expect(page).toHaveURL(noteUrl("Сеть/SSH"));
  } finally {
    rmSync(study, { recursive: true, force: true });
    rmSync(work, { recursive: true, force: true });
  }
});

test("хранилище: переименовать (вкладки — за ним) и удалить в корзину", async ({ page }) => {
  const old = join(VAULT, "..", "Черновики");
  const renamed = join(VAULT, "..", "Черновики 2026");
  mkdirSync(old, { recursive: true });
  writeFileSync(join(old, "Идея.typ"), `${head("Идея")}Текст.\n`);
  try {
    await page.goto(`/v/${encodeURIComponent("Черновики")}/n/${encodeURIComponent("Идея")}`);
    await ready(page);

    await page.locator("#vault-switch").click();
    await page.locator("#vault-rename").click();
    const dialog = page.locator("#vault-edit");
    await expect(dialog.getByLabel("Название")).toHaveValue("Черновики");
    // Занятое имя — ошибка сервера, ничего не переименовано.
    await dialog.getByLabel("Название").fill("vault");
    await dialog.getByRole("button", { name: "Переименовать" }).click();
    await expect(dialog.getByRole("alert")).toContainText("vault");
    await dialog.getByLabel("Название").fill("Черновики 2026");
    await dialog.getByRole("button", { name: "Переименовать" }).click();

    // Та же заметка — под новым именем хранилища.
    await expect(page).toHaveURL(`/v/${encodeURIComponent("Черновики 2026")}/n/${encodeURIComponent("Идея")}`);
    await ready(page);
    await expect(page.locator("#vault-name")).toHaveText("Черновики 2026");
    await expect(page.locator("#note .k-title h1")).toHaveText("Идея");
    expect(existsSync(join(renamed, "Идея.typ"))).toBe(true);
    expect(existsSync(old)).toBe(false);

    // Удалить: «Отмена» — по умолчанию; удалённое — в корзине, дальше — выбор хранилища.
    await page.locator("#vault-switch").click();
    await page.locator("#vault-delete").click();
    await expect(dialog).toContainText("Удалить хранилище «Черновики 2026»?");
    await expect(dialog.getByRole("button", { name: "Отмена" })).toBeFocused();
    await page.locator("#vault-delete-confirm").click();
    await expect(page).toHaveURL("/");
    await ready(page);
    await expect(page.locator("#vault-picker")).toBeVisible();
    await expect(page.locator("#vault-picker").getByRole("link", { name: "Черновики 2026" })).toHaveCount(0);
    expect(existsSync(renamed)).toBe(false);
    expect(existsSync(join(TRASH, "Черновики 2026", "Идея.typ"))).toBe(true);
  } finally {
    rmSync(old, { recursive: true, force: true });
    rmSync(renamed, { recursive: true, force: true });
    rmSync(TRASH, { recursive: true, force: true });
  }
});

test("удалить заметку: из дерева и командой — в корзину, с подтверждением", async ({ page }) => {
  const dir = join(VAULT, "Удаление");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "Черновик.typ"), `${head("Черновик")}Текст.\n`);
  writeFileSync(join(dir, "Второй.typ"), `${head("Второй")}См. #see("Удаление/Черновик").\n`);
  try {
    await open(page, "Удаление/Черновик");
    const link = page.locator("#tree").getByRole("link", { name: "Черновик" });
    const dialog = page.locator("#note-delete");

    // Правый клик в дереве → «Удалить…» → подтверждение; «Отмена» ничего не трогает.
    await link.click({ button: "right" });
    await page.locator("#note-menu").getByRole("menuitem", { name: "Удалить…" }).click();
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText("Удалить заметку «Черновик»?");
    await expect(dialog).toContainText("ссылается 1 заметка");
    await dialog.getByRole("button", { name: "Отмена" }).click();
    await expect(dialog).toBeHidden();
    expect(existsSync(join(dir, "Черновик.typ"))).toBe(true);

    await link.click({ button: "right" });
    await page.locator("#note-menu").getByRole("menuitem", { name: "Удалить…" }).click();
    await page.locator("#note-delete-confirm").click();
    await expect(dialog).toBeHidden();
    await expect(link).toHaveCount(0);
    // Её вкладка закрылась — на месте главная.
    await expect(page).toHaveURL(vaultUrl("/"));
    await expect(page.locator("#status")).toContainText("в корзине: Черновик");
    expect(existsSync(join(dir, "Черновик.typ"))).toBe(false);
    expect(existsSync(join(TRASH, "Черновик.typ"))).toBe(true);

    // Команда палитры — открытую заметку.
    await open(page, "Удаление/Второй");
    await page.keyboard.press("Control+KeyK");
    await page.locator(".palette input").fill(">Удалить заметку");
    await page.keyboard.press("Enter");
    await expect(dialog).toContainText("Удалить заметку «Второй»?");
    await page.locator("#note-delete-confirm").click();
    await expect(page.locator("#tree").getByRole("link", { name: "Второй" })).toHaveCount(0);
    expect(existsSync(join(TRASH, "Второй.typ"))).toBe(true);
    await ready(page);
  } finally {
    rmSync(dir, { recursive: true, force: true });
    rmSync(TRASH, { recursive: true, force: true });
  }
});

test("удалить папку: правый клик в дереве — со всем, что в ней, в корзину", async ({ page }) => {
  const dir = join(VAULT, "Черновики");
  mkdirSync(join(dir, "Старое"), { recursive: true });
  writeFileSync(join(dir, "Первый.typ"), `${head("Первый")}Текст.\n`);
  writeFileSync(join(dir, "Старое", "Второй.typ"), `${head("Второй")}Текст.\n`);
  writeFileSync(join(VAULT, "Ссылка.typ"), `${head("Ссылка")}См. #see("Черновики/Старое/Второй").\n`);
  try {
    await open(page, "Черновики/Старое/Второй");
    const folder = page.locator("#tree summary", { hasText: "Черновики" });
    const dialog = page.locator("#note-delete");

    await folder.click({ button: "right" });
    await page.locator("#note-menu").getByRole("menuitem", { name: "Удалить…" }).click();
    await expect(dialog).toContainText("Удалить папку «Черновики»?");
    await expect(dialog).toContainText("2 заметки");
    await expect(dialog).toContainText("ссылается 1 заметка");
    await expect(dialog.getByRole("button", { name: "Отмена" })).toBeFocused();
    await page.locator("#note-delete-confirm").click();
    await expect(dialog).toBeHidden();
    await expect(folder).toHaveCount(0);
    // Вкладка заметки из папки закрылась.
    await expect(page).toHaveURL(vaultUrl("/"));
    await expect(page.locator("#status")).toContainText("в корзине: папка Черновики");
    expect(existsSync(dir)).toBe(false);
    expect(existsSync(join(TRASH, "Черновики", "Старое", "Второй.typ"))).toBe(true);
    await ready(page);
  } finally {
    rmSync(dir, { recursive: true, force: true });
    rmSync(join(VAULT, "Ссылка.typ"), { force: true });
    rmSync(TRASH, { recursive: true, force: true });
  }
});

test("пустая папка — в дереве; переименовать заметку и папку: файл, название, ссылки, вкладка", async ({ page }) => {
  const empty = join(VAULT, "Пустая");
  const dir = join(VAULT, "Переим");
  mkdirSync(empty, { recursive: true });
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "Старое.typ"), `${head("Старое")}Текст.\n`);
  writeFileSync(join(VAULT, "Ссылка2.typ"), `${head("Ссылка2")}См. #see("Переим/Старое").\n`);
  try {
    await open(page, "Переим/Старое");
    const tree = page.locator("#tree");
    await expect(tree.locator("summary", { hasText: "Пустая" })).toContainText("0");
    const dialog = page.locator("#note-rename");

    // Заметка: план по ходу ввода — новый файл и чьи ссылки поправятся.
    await tree.getByRole("link", { name: "Старое" }).click({ button: "right" });
    await page.locator("#note-menu").getByRole("menuitem", { name: "Переименовать…" }).click();
    const input = dialog.getByLabel("Название");
    await expect(input).toHaveValue("Старое");
    await input.fill("Новое: имя");
    await expect(dialog).toContainText("Переим/Новое имя.typ");
    await expect(dialog).toContainText("Перепишется 1 ссылка в 1 заметке");
    await expect(dialog).toContainText("Ссылка2");
    await page.locator("#note-rename-confirm").click();
    await expect(dialog).toBeHidden();
    await expect(page).toHaveURL(noteUrl("Переим/Новое имя"));
    await expect(page.locator("#note .k-title h1")).toHaveText("Новое: имя");
    expect(existsSync(join(dir, "Старое.typ"))).toBe(false);
    expect(readFileSync(join(VAULT, "Ссылка2.typ"), "utf8")).toContain('#see("Переим/Новое имя")');

    // Папка: всё внутри переезжает, открытая заметка — по новому адресу.
    await tree.locator("summary", { hasText: "Переим" }).click({ button: "right" });
    await page.locator("#note-menu").getByRole("menuitem", { name: "Переименовать…" }).click();
    await input.fill("Готово");
    await expect(dialog).toContainText("Переим/ → Готово/");
    await page.locator("#note-rename-confirm").click();
    await expect(page).toHaveURL(noteUrl("Готово/Новое имя"));
    await expect(tree.locator("summary", { hasText: "Готово" })).toBeVisible();
    expect(existsSync(join(VAULT, "Готово", "Новое имя.typ"))).toBe(true);
    expect(readFileSync(join(VAULT, "Ссылка2.typ"), "utf8")).toContain('#see("Готово/Новое имя")');
    await ready(page);
  } finally {
    for (const p of [empty, dir, join(VAULT, "Готово"), join(VAULT, "Ссылка2.typ")]) rmSync(p, { recursive: true, force: true });
  }
});
