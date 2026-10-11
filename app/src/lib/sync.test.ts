import { expect, it } from "vitest";
import type { SyncVault } from "./api";
import { actionMessage, ago, conflictText, heldText, isBusy, isInsecure, loginMessage, serverLabel, serverOnly, stateText } from "./sync";

const vault = (over: Partial<SyncVault> = {}): SyncVault => ({
  name: "v",
  local: true,
  remote: true,
  linked: true,
  state: "idle",
  last_sync: null,
  error: null,
  report: null,
  held: null,
  ...over,
});

const report = (conflicts: string[]) => ({ uploaded: 0, downloaded: 0, removed_local: 0, removed_remote: 0, conflicts, skipped: [] });

it("ago: just now, minutes, hours, days with the right plural", () => {
  expect(ago(0)).toBe("только что");
  expect(ago(-5)).toBe("только что");
  expect(ago(59)).toBe("только что");
  expect(ago(60)).toBe("1 мин назад");
  expect(ago(59 * 60 + 59)).toBe("59 мин назад");
  expect(ago(3600)).toBe("1 ч назад");
  expect(ago(23 * 3600 + 3000)).toBe("23 ч назад");
  expect(ago(86400)).toBe("1 день назад");
  expect(ago(2 * 86400)).toBe("2 дня назад");
  expect(ago(5 * 86400)).toBe("5 дней назад");
  expect(ago(21 * 86400)).toBe("21 день назад");
});

it("stateText: every state in words", () => {
  const now = 10_000;
  expect(stateText(vault({ linked: false }), now)).toBe("не синхронизируется");
  expect(stateText(vault({ linked: false, local: false }), now)).toBe("только на сервере - включите, чтобы скачать");
  expect(stateText(vault({ last_sync: now - 300 }), now)).toBe("синхронизировано 5 мин назад");
  expect(stateText(vault({ last_sync: now - 3 }), now)).toBe("синхронизировано только что");
  expect(stateText(vault(), now)).toBe("ждёт первой синхронизации");
  expect(stateText(vault({ state: "syncing" }), now)).toBe("идёт синхронизация");
  expect(stateText(vault({ state: "offline" }), now)).toBe("нет связи с сервером");
  expect(stateText(vault({ state: "sign-in" }), now)).toBe("нужен вход");
  expect(stateText(vault({ state: "error", error: "disk full" }), now)).toBe("ошибка: disk full");
  expect(stateText(vault({ state: "error" }), now)).toBe("ошибка");
  expect(stateText(vault({ state: "held" }), now)).toBe("приостановлено - нужно подтверждение");
});

it("isBusy and serverOnly", () => {
  expect(isBusy([vault(), vault({ state: "error" })])).toBe(false);
  expect(isBusy([vault(), vault({ state: "syncing" })])).toBe(true);
  expect(isBusy([])).toBe(false);
  expect(serverOnly(vault({ local: false, linked: false }))).toBe(true);
  expect(serverOnly(vault({ local: false, linked: true }))).toBe(false);
  expect(serverOnly(vault())).toBe(false);
});

it("conflictText: the winner follows device.sync_prefer, a long list is cut", () => {
  expect(conflictText(vault(), "local")).toBeNull();
  expect(conflictText(vault({ report: report([]) }), "local")).toBeNull();
  const v = vault({ report: report(["a.typ", "b.typ"]) });
  expect(conflictText(v, "local")).toBe("Конфликты: a.typ, b.typ - победила версия этого устройства");
  expect(conflictText(v, "remote")).toBe("Конфликты: a.typ, b.typ - победила версия с сервера");
  expect(conflictText(v, undefined)).toContain("этого устройства");
  const many = vault({ report: report(["1", "2", "3", "4", "5"]) });
  expect(conflictText(many, "remote")).toBe("Конфликты: 1, 2, 3 и ещё 2 - победила версия с сервера");
});

it("isInsecure: plain http to another computer only (the core's rule)", () => {
  expect(isInsecure("http://notes.example.org")).toBe(true);
  expect(isInsecure("HTTP://192.168.1.5:8422")).toBe(true);
  expect(isInsecure("http://127.0.0.1:8461")).toBe(false);
  expect(isInsecure("http://localhost:8422/x")).toBe(false);
  expect(isInsecure("http://[::1]:8422")).toBe(false);
  expect(isInsecure("https://notes.example.org")).toBe(false);
  expect(isInsecure("notes.example.org")).toBe(false);
  expect(isInsecure(null)).toBe(false);
});

it("loginMessage: plain Russian for the known codes, the server text for the rest", () => {
  expect(loginMessage({ status: 422, message: "wrong login or password" })).toBe("Неверный логин или пароль");
  expect(loginMessage({ status: 502, message: "cannot reach the server http://x: refused" })).toContain("недоступен");
  expect(loginMessage({ status: 502, message: "the hub answered 429: too many attempts" })).toBe("Слишком много попыток, подождите немного");
  expect(loginMessage({ status: 502, message: "the hub answered 500: boom" })).toBe("the hub answered 500: boom");
  expect(loginMessage({ status: 400, message: "invalid server address" })).toBe("Неверный адрес сервера");
  expect(loginMessage({ status: 0, message: "Failed to fetch" })).toBe("Нет связи с приложением");
  expect(loginMessage({ status: 500, message: "boom" })).toBe("boom");
  expect(loginMessage({ status: 500, message: "" })).toBe("Не удалось войти");
});

it("actionMessage: the 409 cases of link and sync-now", () => {
  expect(actionMessage({ status: 409, message: "not signed in: notes sync login <server>" })).toBe("Нужен вход");
  expect(actionMessage({ status: 409, message: "the session ended: sign in again" })).toBe("Сессия закончилась - войдите снова");
  expect(actionMessage({ status: 409, message: 'sync of "a" is already running' })).toBe("Синхронизация уже идёт");
  expect(actionMessage({ status: 409, message: 'vault "a" is not linked' })).toBe('vault "a" is not linked');
  expect(actionMessage({ status: 502, message: "cannot reach the server x: y" })).toBe("Сервер хранилища недоступен");
  expect(actionMessage({ status: 502, message: "the hub answered 500: boom" })).toBe("the hub answered 500: boom");
  expect(actionMessage({ status: 0, message: "x" })).toBe("Нет связи с приложением");
});

it("serverLabel drops https:// only", () => {
  expect(serverLabel("https://notes.example.org")).toBe("notes.example.org");
  expect(serverLabel("http://127.0.0.1:8461")).toBe("http://127.0.0.1:8461");
  expect(serverLabel("https://example.org/")).toBe("example.org");
  expect(serverLabel(null)).toBe("");
});

it("heldText: what waits, by the side", () => {
  expect(heldText(vault())).toBeNull();
  expect(heldText(vault({ state: "held", held: null }))).toBeNull();
  const server = vault({ state: "held", held: { side: "server", count: 120, total: 130 } });
  expect(heldText(server)).toBe("Пропали 120 из 130 файлов. На сервере они пока целы - удалить их там или вернуть сюда?");
  expect(heldText(vault({ state: "held", held: { side: "server", count: 12, total: 12 } }))).toContain("12 из 12 файлов");
  expect(heldText(vault({ state: "held", held: { side: "device", count: 30, total: 31 } }))).toContain("На сервере удалены 30 из 31 файла");
});

it("actionMessage: a paused sync and nothing to confirm", () => {
  expect(actionMessage({ status: 409, message: 'sync of "v" is paused: 12 of 12 files are gone' })).toContain("приостановлена");
  expect(actionMessage({ status: 409, message: 'nothing of "v" waits for a confirmation' })).toBe("Подтверждать уже нечего");
});
