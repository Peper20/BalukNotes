// Хранилища (создать, переключиться — у каждого свои заметки) и удаление
// заметок из интерфейса: в корзину (у сервера e2e — каталог TRASH), с
// подтверждением; вкладки удалённой заметки закрываются.
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
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
    await expect(menu.getByRole("menuitem", { name: "Учёба" })).toHaveAttribute("aria-current", "true");
    await menu.getByRole("menuitem", { name: "vault" }).click();
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
