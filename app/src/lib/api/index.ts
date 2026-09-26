// Запросы к серверу notes. Типы ответов — из Rust (types/, `npm run types`).

import { encodeId } from "../ids";
import type { ErrorResponse } from "./types/ErrorResponse";
import type { Graph } from "./types/Graph";
import type { LinksResponse } from "./types/LinksResponse";
import type { NoteListItem } from "./types/NoteListItem";
import type { NotePage } from "./types/NotePage";
import type { Preview } from "./types/Preview";
import type { SearchHit } from "./types/SearchHit";
import type { SettingsResponse } from "./types/SettingsResponse";
import type { Theme } from "./types/Theme";
import type { VersionResponse } from "./types/VersionResponse";

export type { BookView } from "./types/BookView";
export type { Chapter } from "./types/Chapter";
export type { Diagnostic } from "./types/Diagnostic";
export type { Graph } from "./types/Graph";
export type { Heading } from "./types/Heading";
export type { LinksResponse } from "./types/LinksResponse";
export type { NoteListItem } from "./types/NoteListItem";
export type { NotePage } from "./types/NotePage";
export type { Preview } from "./types/Preview";
export type { Rendered } from "./types/Rendered";
export type { SearchHit } from "./types/SearchHit";
export type { Schema } from "./types/Schema";
export type { SettingDef } from "./types/SettingDef";
export type { Theme } from "./types/Theme";

/** Какую главу книги запросить: по номеру или ту, где якорь. */
export interface ChapterSelect {
  chapter?: number | null;
  anchor?: string | null;
}

/** Значения настроек: ключ → число, строка или флаг. */
export type SettingValues = SettingsResponse["values"];

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly body?: ErrorResponse,
  ) {
    super(message);
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, init);
  const body: unknown = await res.json().catch(() => null);
  if (!res.ok) {
    const err = body as ErrorResponse | null;
    throw new ApiError(err?.error || `${res.status} ${res.statusText}`, res.status, err ?? undefined);
  }
  return body as T;
}

export const api = {
  notes: () => request<NoteListItem[]>("/api/notes"),
  /** Заметка; с `chapter` или `anchor` книга приходит одной главой. */
  note: (id: string, signal?: AbortSignal, select: ChapterSelect = {}) => {
    const q = select.chapter != null ? `?chapter=${select.chapter}` : select.anchor != null ? `?anchor=${encodeURIComponent(select.anchor)}` : "";
    return request<NotePage>(`/api/notes/${encodeId(id)}${q}`, { signal });
  },
  version: (id: string) => request<VersionResponse>(`/api/version/${encodeId(id)}`),
  links: (id: string) => request<LinksResponse>(`/api/links/${encodeId(id)}`),
  graph: () => request<Graph>("/api/graph"),
  search: (q: string, signal?: AbortSignal, limit = 30) =>
    request<SearchHit[]>(`/api/search?q=${encodeURIComponent(q)}&limit=${limit}`, { signal }),
  preview: (id: string, anchor?: string | null, signal?: AbortSignal) =>
    request<Preview>(`/api/preview/${encodeId(id)}${anchor ? `?anchor=${encodeURIComponent(anchor)}` : ""}`, { signal }),
  themes: () => request<Theme[]>("/api/themes"),
  settings: () => request<SettingsResponse>("/api/settings"),
  saveSettings: (patch: SettingValues) =>
    request<SettingValues>("/api/settings", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(patch),
    }),
  pdfUrl: (id: string, theme: string) => `/api/pdf/${encodeId(id)}?theme=${encodeURIComponent(theme)}`,
};
