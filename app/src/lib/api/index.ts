// Запросы к серверу notes. Типы ответов — из Rust (types/, `npm run types`).
// Адрес сервера и токен — config.ts; ошибки сети и сервера — ApiError.

import { encodeId } from "../ids";
import { vault } from "../vault";
import { apiUrl, authHeaders } from "./config";
import type { ErrorResponse } from "./types/ErrorResponse";
import type { FolderListItem } from "./types/FolderListItem";
import type { Graph } from "./types/Graph";
import type { GraphFilter } from "./types/GraphFilter";
import type { GraphLayout } from "./types/GraphLayout";
import type { LinksResponse } from "./types/LinksResponse";
import type { NoteListItem } from "./types/NoteListItem";
import type { NotePage } from "./types/NotePage";
import type { RenamePlan } from "./types/RenamePlan";
import type { RenameRequest } from "./types/RenameRequest";
import type { Preview } from "./types/Preview";
import type { SearchHit } from "./types/SearchHit";
import type { SettingsResponse } from "./types/SettingsResponse";
import type { Theme } from "./types/Theme";
import type { VaultSettingsResponse } from "./types/VaultSettingsResponse";
import type { VersionResponse } from "./types/VersionResponse";
import type { VaultsResponse } from "./types/VaultsResponse";
import type { WarmRequest } from "./types/WarmRequest";

export { apiConfig, apiUrl, configure, rebaseStylesheets, type ApiConfig } from "./config";
export type { BookView } from "./types/BookView";
export type { Chapter } from "./types/Chapter";
export type { Diagnostic } from "./types/Diagnostic";
export type { Graph } from "./types/Graph";
export type { GraphFilter } from "./types/GraphFilter";
export type { GraphLayout } from "./types/GraphLayout";
export type { PlacedNode } from "./types/PlacedNode";
export type { Heading } from "./types/Heading";
export type { Backlink } from "./types/Backlink";
export type { LinksResponse } from "./types/LinksResponse";
export type { NoteListItem } from "./types/NoteListItem";
export type { FolderListItem } from "./types/FolderListItem";
export type { NotePage } from "./types/NotePage";
export type { RenamePlan } from "./types/RenamePlan";
export type { Preview } from "./types/Preview";
export type { Rendered } from "./types/Rendered";
export type { SearchHit } from "./types/SearchHit";
export type { TaggedChapter } from "./types/TaggedChapter";
export type { Fragment } from "./types/Fragment";
export type { Schema } from "./types/Schema";
export type { SettingDef } from "./types/SettingDef";
export type { Apply } from "./types/Apply";
export type { Theme } from "./types/Theme";
export type { VaultsResponse } from "./types/VaultsResponse";
export type { VaultSettingsResponse } from "./types/VaultSettingsResponse";

/** Какую главу книги запросить: по номеру или ту, где якорь. */
export interface ChapterSelect {
  chapter?: number | null;
  anchor?: string | null;
}

/** Значения настроек: ключ → число, строка или флаг. */
export type SettingValues = SettingsResponse["values"];

/**
 * Ошибка запроса — одним типом для сети и сервера: `status` — код ответа,
 * 0 — сервер недоступен (сеть). Отмена запроса (`AbortSignal`) — не
 * ApiError, а обычный `AbortError`.
 */
export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly body?: ErrorResponse,
  ) {
    super(message);
  }

  /** Сервер не ответил (нет сети, сервер остановлен). */
  get offline(): boolean {
    return this.status === 0;
  }
}

/** Кто следит за связью с сервером (state/connection): ответил ли он на запрос. */
let reached: (ok: boolean) => void = () => {};

/** Сообщать `fn`, ответил ли сервер на очередной запрос (ответ с ошибкой — тоже ответ). */
export function onReach(fn: (ok: boolean) => void): void {
  reached = fn;
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  let res: Response;
  try {
    res = await fetch(apiUrl(path), { ...init, headers: { ...authHeaders(), ...(init.headers as Record<string, string> | undefined) } });
  } catch (e) {
    if ((e as Error).name === "AbortError") throw e;
    reached(false);
    throw new ApiError((e as Error).message, 0);
  }
  reached(true);
  const body: unknown = await res.json().catch(() => null);
  if (!res.ok) {
    const err = body as ErrorResponse | null;
    throw new ApiError(err?.error || `${res.status} ${res.statusText}`, res.status, err ?? undefined);
  }
  return body as T;
}

const json = (method: string, data: unknown): RequestInit => ({ method, headers: { "Content-Type": "application/json" }, body: JSON.stringify(data) });

/** Путь API показанного хранилища: `/api/vaults/<имя><path>`. */
const inVault = (path: string): string => {
  const name = vault();
  if (name == null) throw new Error("хранилище не выбрано");
  return `/api/vaults/${encodeURIComponent(name)}${path}`;
};

export const api = {
  /** Хранилища и какое открыть по умолчанию. */
  vaults: () => request<VaultsResponse>("/api/vaults"),
  /** Новое пустое хранилище; ответ — список хранилищ. */
  createVault: (name: string) => request<VaultsResponse>("/api/vaults", json("POST", { name })),
  /** Переименовать хранилище (папку); ответ — список хранилищ. */
  renameVault: (from: string, name: string) => request<VaultsResponse>(`/api/vaults/${encodeURIComponent(from)}`, json("PATCH", { name })),
  /** Хранилище целиком — в корзину системы; ответ — список хранилищ. */
  deleteVault: (name: string) => request<VaultsResponse>(`/api/vaults/${encodeURIComponent(name)}`, { method: "DELETE" }),
  notes: () => request<NoteListItem[]>(inVault("/notes")),
  folders: () => request<FolderListItem[]>(inVault("/folders")),
  /** Заметка; с `chapter` или `anchor` книга приходит одной главой. */
  note: (id: string, signal?: AbortSignal, select: ChapterSelect = {}) => {
    const q = select.chapter != null ? `?chapter=${select.chapter}` : select.anchor != null ? `?anchor=${encodeURIComponent(select.anchor)}` : "";
    return request<NotePage>(inVault(`/notes/${encodeId(id)}${q}`), { signal });
  },
  /** Удалить заметку (книгу — папкой) — в корзину системы. */
  deleteNote: (id: string) => request<unknown>(inVault(`/notes/${encodeId(id)}`), { method: "DELETE" }).then(() => {}),
  /** Переименовать заметку (книгу) или папку; без `apply` — только план. */
  rename: (r: RenameRequest, signal?: AbortSignal) => request<RenamePlan>(inVault("/rename"), { ...json("POST", r), signal }),
  /** Удалить папку со всем, что в ней, — в корзину системы. */
  deleteFolder: (path: string) => request<unknown>(inVault(`/folders/${encodeId(path)}`), { method: "DELETE" }).then(() => {}),
  version: (id: string) => request<VersionResponse>(inVault(`/version/${encodeId(id)}`)),
  links: (id: string) => request<LinksResponse>(inVault(`/links/${encodeId(id)}`)),
  graph: () => request<Graph>(inVault("/graph")),
  /** Граф по фильтру, уже разложенный (фильтр и раскладка — в ядре). */
  graphLayout: (filter: Partial<GraphFilter> = {}) => request<GraphLayout>(inVault("/graph/layout"), json("POST", filter)),
  /** Поиск по тексту всех заметок; с `note` — только в ней (все разделы по порядку). */
  search: (q: string, signal?: AbortSignal, limit = 30, note?: string | null) =>
    request<SearchHit[]>(inVault(`/search?q=${encodeURIComponent(q)}&limit=${limit}${note ? `&note=${encodeURIComponent(note)}` : ""}`), { signal }),
  preview: (id: string, anchor?: string | null, signal?: AbortSignal) =>
    request<Preview>(inVault(`/preview/${encodeId(id)}${anchor ? `?anchor=${encodeURIComponent(anchor)}` : ""}`), { signal }),
  themes: () => request<Theme[]>("/api/themes"),
  settings: () => request<SettingsResponse>("/api/settings"),
  saveSettings: (patch: SettingValues) => request<SettingValues>("/api/settings", json("PUT", patch)),
  /** Настройки показанного хранилища: итог, общие и заданные в нём. */
  vaultSettings: () => request<VaultSettingsResponse>(inVault("/settings")),
  /** Задать настройки только для показанного хранилища; `null` — снова общая. */
  saveVaultSettings: (patch: Record<string, number | string | boolean | null>) =>
    request<VaultSettingsResponse>(inVault("/settings"), json("PUT", patch)),
  /** Подсказать серверу, что собрать заранее первым (ответ не нужен). */
  warm: (req: WarmRequest) => request<unknown>(inVault("/warm"), json("POST", req)).then(() => {}),
  /** Адрес PDF — его открывает браузер (новая вкладка), токен — в адресе. */
  pdfUrl: (id: string, theme: string) => apiUrl(inVault(`/pdf/${encodeId(id)}?theme=${encodeURIComponent(theme)}`), { withToken: true }),
  /** Адрес потока событий хранилища (`EventSource`), токен — в адресе. */
  eventsUrl: () => apiUrl(inVault("/events"), { withToken: true }),
};
