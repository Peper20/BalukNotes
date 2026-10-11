// Requests to the notes server. Response types come from Rust (types/,
// `npm run types`). Server address - config.ts; network and server errors -
// ApiError; a 401 (sign-in needed) - signin.ts.

import { encodeId } from "../ids";
import { vault } from "../vault";
import { apiUrl } from "./config";
import { requireSignIn } from "./signin";
import type { ErrorResponse } from "./types/ErrorResponse";
import type { EventsResponse } from "./types/EventsResponse";
import type { FolderListItem } from "./types/FolderListItem";
import type { Graph } from "./types/Graph";
import type { GraphFilter } from "./types/GraphFilter";
import type { GraphLayout } from "./types/GraphLayout";
import type { LinksResponse } from "./types/LinksResponse";
import type { LoginRequest } from "./types/LoginRequest";
import type { NoteListItem } from "./types/NoteListItem";
import type { NotePage } from "./types/NotePage";
import type { RenamePlan } from "./types/RenamePlan";
import type { RenameRequest } from "./types/RenameRequest";
import type { Preview } from "./types/Preview";
import type { SearchHit } from "./types/SearchHit";
import type { SessionResponse } from "./types/SessionResponse";
import type { SettingsResponse } from "./types/SettingsResponse";
import type { SyncAccount } from "./types/SyncAccount";
import type { SyncLogin } from "./types/SyncLogin";
import type { SyncReport } from "./types/SyncReport";
import type { SyncStatus } from "./types/SyncStatus";
import type { Theme } from "./types/Theme";
import type { VaultSettingsResponse } from "./types/VaultSettingsResponse";
import type { VersionResponse } from "./types/VersionResponse";
import type { VaultsResponse } from "./types/VaultsResponse";
import type { WarmRequest } from "./types/WarmRequest";

export { apiConfig, apiUrl, configure, rebaseStylesheets, type ApiConfig } from "./config";
export { isSignIn, onSignInRequired, signInMessage, signInRequired } from "./signin";
export type { SessionResponse } from "./types/SessionResponse";
export type { BookView } from "./types/BookView";
export type { Chapter } from "./types/Chapter";
export type { Diagnostic } from "./types/Diagnostic";
export type { Graph } from "./types/Graph";
export type { Forces } from "./types/Forces";
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
export type { SyncAccount } from "./types/SyncAccount";
export type { SyncHeld } from "./types/SyncHeld";
export type { SyncReport } from "./types/SyncReport";
export type { SyncState } from "./types/SyncState";
export type { SyncStatus } from "./types/SyncStatus";
export type { SyncVault } from "./types/SyncVault";

/** Which book chapter to request: by number or the one with the anchor. */
export interface ChapterSelect {
  chapter?: number | null;
  anchor?: string | null;
}

/** Setting values: key -> number, string or flag. */
export type SettingValues = SettingsResponse["values"];

/**
 * A request error, one type for the network and the server: `status` is the
 * response code, 0 - the server is unreachable (network). A cancelled request
 * (`AbortSignal`) is not an ApiError but a plain `AbortError`.
 * `retryAfter` is the `Retry-After` header in seconds (429).
 */
export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly body?: ErrorResponse,
    readonly retryAfter: number | null = null,
  ) {
    super(message);
  }

  /** The server did not answer (no network, the server is stopped). */
  get offline(): boolean {
    return this.status === 0;
  }
}

/** Who watches the connection to the server (state/connection): whether it answered a request. */
let reached: (ok: boolean) => void = () => {};

/** Tells `fn` whether the server answered the next request (an error response is an answer too). */
export function onReach(fn: (ok: boolean) => void): void {
  reached = fn;
}

/**
 * One request. The cookie of the session goes with it (also to another
 * configured address). A 401 means "sign-in needed" for the whole client
 * (`signin.ts`), except for the sign-in request itself (`signIn: false`).
 */
async function request<T>(path: string, init: RequestInit = {}, { signIn = true } = {}): Promise<T> {
  let res: Response;
  try {
    res = await fetch(apiUrl(path), { credentials: "include", ...init });
  } catch (e) {
    if ((e as Error).name === "AbortError") throw e;
    reached(false);
    throw new ApiError((e as Error).message, 0);
  }
  reached(true);
  const body: unknown = await res.json().catch(() => null);
  if (!res.ok) {
    if (res.status === 401 && signIn) requireSignIn();
    const err = body as ErrorResponse | null;
    const wait = Number.parseInt(res.headers.get("Retry-After") ?? "", 10);
    throw new ApiError(err?.error || `${res.status} ${res.statusText}`, res.status, err ?? undefined, Number.isFinite(wait) ? wait : null);
  }
  return body as T;
}

const json = (method: string, data: unknown): RequestInit => ({ method, headers: { "Content-Type": "application/json" }, body: JSON.stringify(data) });

/** API path of the shown vault: `/api/vaults/<name><path>`. */
const inVault = (path: string): string => {
  const name = vault();
  if (name == null) throw new Error("no vault chosen");
  return `/api/vaults/${encodeURIComponent(name)}${path}`;
};

export const api = {
  /**
   * The signed-in login; `null` - this server has no sign-in (the window's
   * socket, `notes serve` on localhost: 204, an older server: 404). Without a
   * session on a server with sign-in - ApiError 401.
   */
  session: () =>
    request<SessionResponse | null>("/api/session").then(
      (s) => s?.login ?? null,
      (e: unknown) => {
        if (e instanceof ApiError && e.status === 404) return null;
        throw e;
      },
    ),
  /** Sign in; the server sets the session cookie. A wrong password is ApiError 401 (no sign-in screen change), too many attempts - 429. */
  login: (login: string, password: string) =>
    request<unknown>("/api/login", json("POST", { login, password } satisfies LoginRequest), { signIn: false }).then(() => {}),
  /** Ends the session of this browser. */
  logout: () => request<unknown>("/api/logout", { method: "POST" }, { signIn: false }).then(() => {}),
  /** Vaults and which one to open by default. */
  vaults: () => request<VaultsResponse>("/api/vaults"),
  /** A new empty vault; the response is the vault list. */
  createVault: (name: string) => request<VaultsResponse>("/api/vaults", json("POST", { name })),
  /** Renames a vault (its folder); the response is the vault list. */
  renameVault: (from: string, name: string) => request<VaultsResponse>(`/api/vaults/${encodeURIComponent(from)}`, json("PATCH", { name })),
  /** Moves a whole vault to the system trash; the response is the vault list. */
  deleteVault: (name: string) => request<VaultsResponse>(`/api/vaults/${encodeURIComponent(name)}`, { method: "DELETE" }),
  notes: () => request<NoteListItem[]>(inVault("/notes")),
  folders: () => request<FolderListItem[]>(inVault("/folders")),
  /** A note; with `chapter` or `anchor` a book comes as one chapter. */
  note: (id: string, signal?: AbortSignal, select: ChapterSelect = {}) => {
    const q = select.chapter != null ? `?chapter=${select.chapter}` : select.anchor != null ? `?anchor=${encodeURIComponent(select.anchor)}` : "";
    return request<NotePage>(inVault(`/notes/${encodeId(id)}${q}`), { signal });
  },
  /** Deletes a note (a book as a folder) to the system trash. */
  deleteNote: (id: string) => request<unknown>(inVault(`/notes/${encodeId(id)}`), { method: "DELETE" }).then(() => {}),
  /** Renames a note (book) or a folder; without `apply`, only the plan. */
  rename: (r: RenameRequest, signal?: AbortSignal) => request<RenamePlan>(inVault("/rename"), { ...json("POST", r), signal }),
  /** Deletes a folder with everything in it to the system trash. */
  deleteFolder: (path: string) => request<unknown>(inVault(`/folders/${encodeId(path)}`), { method: "DELETE" }).then(() => {}),
  version: (id: string) => request<VersionResponse>(inVault(`/version/${encodeId(id)}`)),
  links: (id: string) => request<LinksResponse>(inVault(`/links/${encodeId(id)}`)),
  graph: () => request<Graph>(inVault("/graph")),
  /** The graph by the filter, already laid out (filter and layout are in the core). */
  graphLayout: (filter: Partial<GraphFilter> = {}) => request<GraphLayout>(inVault("/graph/layout"), json("POST", filter)),
  /** Full-text search over all notes; with `note`, only in it (all sections in order). */
  search: (q: string, signal?: AbortSignal, limit = 30, note?: string | null) =>
    request<SearchHit[]>(inVault(`/search?q=${encodeURIComponent(q)}&limit=${limit}${note ? `&note=${encodeURIComponent(note)}` : ""}`), { signal }),
  preview: (id: string, anchor?: string | null, signal?: AbortSignal) =>
    request<Preview>(inVault(`/preview/${encodeId(id)}${anchor ? `?anchor=${encodeURIComponent(anchor)}` : ""}`), { signal }),
  themes: () => request<Theme[]>("/api/themes"),
  settings: () => request<SettingsResponse>("/api/settings"),
  saveSettings: (patch: SettingValues) => request<SettingValues>("/api/settings", json("PUT", patch)),
  /** Settings of the shown vault: the result, the shared ones and those set in it. */
  vaultSettings: () => request<VaultSettingsResponse>(inVault("/settings")),
  /** Sets settings only for the shown vault; `null` makes one shared again. */
  saveVaultSettings: (patch: Record<string, number | string | boolean | null>) =>
    request<VaultSettingsResponse>(inVault("/settings"), json("PUT", patch)),
  /**
   * Vault sync of this device: the account and every vault with its state.
   * ApiError 404 - this server runs without sync.
   */
  syncStatus: () => request<SyncStatus>("/api/device/sync"),
  /** Signs in to a storage server. 422: wrong login or password, 502: the server is unreachable (never 401). */
  syncLogin: (r: SyncLogin) => request<SyncAccount>("/api/device/sync/login", json("POST", r)),
  /** Forgets the account; linked vaults stay linked. */
  syncLogout: () => request<unknown>("/api/device/sync/logout", { method: "POST" }).then(() => {}),
  /** Links a vault and runs its first round (a vault only on the server is downloaded). */
  syncLink: (name: string) => request<SyncReport>(`/api/device/sync/vaults/${encodeURIComponent(name)}/link`, { method: "POST" }),
  /** Stops syncing a vault; files stay on both sides. */
  syncUnlink: (name: string) => request<unknown>(`/api/device/sync/vaults/${encodeURIComponent(name)}/unlink`, { method: "POST" }).then(() => {}),
  /** One round of a linked vault now. */
  syncNow: (name: string) => request<SyncReport>(`/api/device/sync/vaults/${encodeURIComponent(name)}/now`, { method: "POST" }),
  /** Deletes the files a held round stopped at (state "held"). 409: nothing waits, or more files are gone now. */
  syncConfirm: (name: string) => request<SyncReport>(`/api/device/sync/vaults/${encodeURIComponent(name)}/confirm`, { method: "POST" }),
  /** Gets back from the storage server the files a held round stopped at, instead of deleting them there. */
  syncRestore: (name: string) => request<SyncReport>(`/api/device/sync/vaults/${encodeURIComponent(name)}/restore`, { method: "POST" }),
  /** Tells the server what to build in advance first (no response needed). */
  warm: (req: WarmRequest) => request<unknown>(inVault("/warm"), json("POST", req)).then(() => {}),
  /** PDF address: the browser opens it (a new tab), the session cookie goes with it. */
  pdfUrl: (id: string, theme: string) => apiUrl(inVault(`/pdf/${encodeId(id)}?theme=${encodeURIComponent(theme)}`)),
  /** Vault changes after `after` (long polling: the response comes on a change or after ~25 s). */
  events: (after: number | null, signal?: AbortSignal) =>
    request<EventsResponse>(inVault(`/events${after == null ? "" : `?after=${after}`}`), { signal }),
};
