// Vault sync in the settings (components/SyncSettings.svelte): pure helpers -
// the state of a vault in words, "5 мин назад", errors in plain Russian.
// The state itself (requests, refreshing) is state/sync.svelte.ts.

import type { SyncVault } from "./api";
import { plural } from "./plural";

/** "только что", "5 мин назад", "3 ч назад", "2 дня назад": `seconds` is how long ago. */
export function ago(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  if (s < 60) return "только что";
  const minutes = Math.floor(s / 60);
  if (minutes < 60) return `${minutes} мин назад`;
  const hours = Math.floor(s / 3600);
  if (hours < 24) return `${hours} ч назад`;
  const days = Math.floor(s / 86400);
  return `${days} ${plural(days, "день", "дня", "дней")} назад`;
}

/** What the row of a vault says about its sync; `now` is Unix seconds. */
export function stateText(v: SyncVault, now: number): string {
  if (!v.linked) return v.local ? "не синхронизируется" : "только на сервере - включите, чтобы скачать";
  switch (v.state) {
    case "syncing":
      return "идёт синхронизация";
    case "offline":
      return "нет связи с сервером";
    case "sign-in":
      return "нужен вход";
    case "held":
      return "приостановлено - нужно подтверждение";
    case "error":
      return v.error ? roundError(v.error) : "ошибка";
    case "idle":
      return v.last_sync == null ? "ждёт первой синхронизации" : `синхронизировано ${ago(now - v.last_sync)}`;
  }
}

/** What a held vault waits for, in words (null: it does not); the buttons follow `v.held.side`. */
export function heldText(v: SyncVault): string | null {
  const held = v.held;
  if (v.state !== "held" || !held) return null;
  const files = `${held.count} из ${held.total} ${plural(held.total, "файла", "файлов", "файлов")}`;
  return held.side === "server"
    ? `Пропали ${files}. На сервере они пока целы - удалить их там или вернуть сюда?`
    : `На сервере удалены ${files}. Здесь они пока целы - убрать их с этого устройства? (Или выключите и включите синхронизацию: файлы загрузятся на сервер.)`;
}

/**
 * A failed round in plain Russian. The core's error is one English line (`notes-device/src/error.rs`,
 * `notes-store/src/sync/error.rs`), recognised by its start; the line itself is shown small under
 * the sentence (`SyncSettings.svelte`), an unknown one gets the general sentence.
 */
export function roundError(error: string): string {
  const rules: [RegExp, string][] = [
    [/^(cannot reach|network error)/, "Нет связи с сервером хранилища"],
    [/^(the session ended|not signed in|unauthorized)/, "Сессия закончилась - войдите снова"],
    [/^wrong login/, "Неверный логин или пароль"],
    [/^invalid server address/, "Неверный адрес сервера"],
    [/^the hub answered 429/, "Сервер хранилища просит подождать: слишком много запросов"],
    [/^the hub answered/, "Сервер хранилища ответил ошибкой"],
    [/is not on the server|^not found:/, "Хранилища нет на сервере - отключите синхронизацию и включите снова"],
    [/^the folder of vault .* is missing/, "Папка хранилища не найдена на устройстве"],
    [/bytes is over the limit/, "Файл слишком большой для синхронизации"],
    [/not a valid file/, "Служебный файл синхронизации повреждён"],
    [/refused a forced write/, "Сервер отказался принять изменения"],
    [/^sync of ".*" is already running/, "Синхронизация уже идёт"],
    [/^\/|No such file|Permission denied|os error/, "Не удалось прочитать или записать файл на устройстве"],
  ];
  const hit = rules.find(([re]) => re.test(error));
  return hit ? hit[1] : "Не удалось синхронизировать";
}

/** The server address as the person knows it: without "https://". */
export const serverLabel = (server: string | null | undefined): string => (server ?? "").replace(/^https:\/\//i, "").replace(/\/$/, "");

/** Something is in progress: the status is worth asking again soon. */
export const isBusy = (vaults: readonly SyncVault[]): boolean => vaults.some((v) => v.state === "syncing");

/** Only on the server: turning the switch on downloads it. */
export const serverOnly = (v: SyncVault): boolean => !v.local && !v.linked;

/** How many conflicting files a row names before "и ещё N". */
const CONFLICTS_SHOWN = 3;

/** "Конфликты: a, b - победила версия этого устройства" (`prefer`: the setting `device.sync_prefer`); null - none. */
export function conflictText(v: SyncVault, prefer: unknown): string | null {
  const files = v.report?.conflicts ?? [];
  if (!files.length) return null;
  const shown = files.slice(0, CONFLICTS_SHOWN).join(", ");
  const rest = files.length > CONFLICTS_SHOWN ? ` и ещё ${files.length - CONFLICTS_SHOWN}` : "";
  const winner = prefer === "remote" ? "с сервера" : "этого устройства";
  return `Конфликты: ${shown}${rest} - победила версия ${winner}`;
}

/** A plain `http://` address to a host that is not this computer (the same rule as the core's `parse_server`). */
export function isInsecure(server: string | null | undefined): boolean {
  const text = (server ?? "").trim();
  const m = /^([a-z][a-z0-9+.-]*):\/\/(.*)$/i.exec(text);
  if (!m || m[1]!.toLowerCase() !== "http") return false;
  const authority = m[2]!.split("/")[0]!;
  const host = authority.startsWith("[") ? authority.slice(1).split("]")[0]! : authority.split(":")[0]!;
  return !(host.toLowerCase() === "localhost" || /^127(\.\d{1,3}){3}$/.test(host) || host === "::1");
}

type ErrorLike = { status?: number; message?: string } | null;

/** The storage server answered but refused: too many attempts have words, the rest is its own text. */
function refused(text: string | undefined): string {
  if (/answered 429/.test(text ?? "")) return "Слишком много попыток, подождите немного";
  return text || "Сервер хранилища недоступен";
}

/** Under the sign-in form: what went wrong, in plain words. */
export function loginMessage(e: unknown): string {
  const err = e as ErrorLike;
  switch (err?.status) {
    case 422:
      return "Неверный логин или пароль";
    case 502:
      // 502 is also "the server answered with a refusal": its own words are shown then.
      return /^cannot reach/.test(err.message ?? "") ? "Сервер хранилища недоступен. Проверьте адрес и сеть" : refused(err.message);
    case 400:
      return "Неверный адрес сервера";
    case 0:
      return "Нет связи с приложением";
    default:
      return err?.message || "Не удалось войти";
  }
}

/** In a vault row: why the switch or the button did not work. */
export function actionMessage(e: unknown): string {
  const err = e as ErrorLike;
  const text = err?.message ?? "";
  switch (err?.status) {
    case 0:
      return "Нет связи с приложением";
    case 502:
      return /^cannot reach/.test(text) ? "Сервер хранилища недоступен" : refused(text);
    case 409:
      if (text.startsWith("not signed in")) return "Нужен вход";
      if (text.startsWith("the session ended")) return "Сессия закончилась - войдите снова";
      if (/^sync of ".*" is already running/.test(text)) return "Синхронизация уже идёт";
      if (/^sync of ".*" is paused/.test(text)) return "Синхронизация приостановлена: подтвердите удаление или верните файлы";
      if (/^nothing of ".*" waits for a confirmation/.test(text)) return "Подтверждать уже нечего";
      return text || "Не удалось";
    default:
      return text || "Не удалось";
  }
}
