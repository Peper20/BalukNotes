// Note addresses in the client: /v/<vault>/n/<path>#<anchor> (the vault -
// ./vault.ts; without it /n/..., as the core puts links in the note HTML).

import { splitVaultPath, vaultBase, vaultHome } from "./vault";

/** Note path in a URL: segments are encoded, "/" stays. */
export const encodeId = (id: string): string => id.split("/").map(encodeURIComponent).join("/");

/** Address of a note (and a section in it). */
export const noteHref = (id: string, anchor?: string | null): string =>
  `${vaultBase()}/n/${encodeId(id)}${anchor ? `#${encodeURIComponent(anchor)}` : ""}`;

/** Home of the shown vault. */
export const homeHref = (): string => vaultHome();

export type Route =
  | { kind: "home" }
  | { kind: "note"; id: string }
  | { kind: "folder"; path: string }
  | { kind: "tags"; tag: string | null }
  | { kind: "graph"; around: string | null; depth: number; folder: string | null };

/** Address of a folder page (path from the vault root). */
export const folderHref = (path: string): string => `${vaultBase()}/f/${encodeId(path)}`;

/** Address of the tags page (or of one tag). */
export const tagHref = (tag?: string | null): string => `${vaultBase()}${tag ? `/tags/${encodeURIComponent(tag)}` : "/tags"}`;

/** Address of the graph: the whole one or the neighbours of a note within `depth` steps. */
export const graphHref = (around?: string | null, depth = 1): string =>
  `${vaultBase()}/graph${around ? `?around=${encodeURIComponent(around)}${depth === 1 ? "" : `&depth=${depth}`}` : ""}`;

/** Address of a folder graph: its notes with subfolders. */
export const folderGraphHref = (path: string): string => `${vaultBase()}/graph?folder=${encodeURIComponent(path)}`;

/**
 * A client address (a note, tags, the graph, home) of the shown vault or
 * without a vault (core links, old addresses); an address of another vault
 * is not: that one goes by a navigation with a reload.
 */
export function isAppPath(pathname: string): boolean {
  const own = vaultBase();
  const split = splitVaultPath(pathname);
  if (split) {
    if (!own || pathname.slice(0, own.length + 1) !== `${own}/` && pathname !== own) return false;
    pathname = split.rest;
  }
  return (
    pathname === "/" ||
    pathname.startsWith("/n/") ||
    pathname.startsWith("/f/") ||
    pathname === "/tags" ||
    pathname.startsWith("/tags/") ||
    pathname === "/graph"
  );
}

/**
 * The route by the page address (`search` for the graph): the vault at the
 * start of the address is skipped. Bad encoding gives home.
 */
export function parseRoute(pathname: string, search = ""): Route {
  pathname = splitVaultPath(pathname)?.rest ?? pathname;
  let path: string;
  try {
    path = decodeURIComponent(pathname);
  } catch {
    return { kind: "home" };
  }
  if (path === "/tags" || path.startsWith("/tags/")) return { kind: "tags", tag: path.slice(6) || null };
  if (path === "/graph" || path === "/graph/") {
    const q = new URLSearchParams(search);
    const depth = Math.round(Number(q.get("depth") ?? 1));
    return { kind: "graph", around: q.get("around") || null, depth: depth >= 1 && depth <= 5 ? depth : 1, folder: q.get("folder") || null };
  }
  const folder = path.startsWith("/f/") ? path.slice(3).replace(/\/+$/, "") : "";
  if (folder) return { kind: "folder", path: folder };
  const id = path.startsWith("/n/") ? path.slice(3).replace(/\/+$/, "") : "";
  return id ? { kind: "note", id } : { kind: "home" };
}

/** The anchor from `location.hash` without `#`; empty - null. */
export function hashAnchor(hash: string): string | null {
  if (hash.length < 2) return null;
  try {
    return decodeURIComponent(hash.slice(1));
  } catch {
    return hash.slice(1);
  }
}

/** Name and folder by id: `Сеть/SSH` -> { name: "SSH", folder: "Сеть" }. */
export function splitId(id: string): { name: string; folder: string } {
  const i = id.lastIndexOf("/");
  return i < 0 ? { name: id, folder: "" } : { name: id.slice(i + 1), folder: id.slice(0, i) };
}
