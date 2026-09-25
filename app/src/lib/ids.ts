// Адреса заметок в клиенте: /n/<путь>#<якорь>.

/** Путь заметки в URL: сегменты кодируются, «/» остаётся. */
export const encodeId = (id: string): string => id.split("/").map(encodeURIComponent).join("/");

/** Адрес заметки (и раздела в ней). */
export const noteHref = (id: string, anchor?: string | null): string =>
  `/n/${encodeId(id)}${anchor ? `#${encodeURIComponent(anchor)}` : ""}`;

export type Route = { kind: "home" } | { kind: "note"; id: string } | { kind: "tags"; tag: string | null };

/** Адрес страницы тегов (или одного тега). */
export const tagHref = (tag?: string | null): string => (tag ? `/tags/${encodeURIComponent(tag)}` : "/tags");

/** Маршрут по адресу страницы. Неверное кодирование — главная. */
export function parseRoute(pathname: string): Route {
  let path: string;
  try {
    path = decodeURIComponent(pathname);
  } catch {
    return { kind: "home" };
  }
  if (path === "/tags" || path.startsWith("/tags/")) return { kind: "tags", tag: path.slice(6) || null };
  const id = path.startsWith("/n/") ? path.slice(3).replace(/\/+$/, "") : "";
  return id ? { kind: "note", id } : { kind: "home" };
}

/** Якорь из `location.hash` без `#`; пустой — null. */
export function hashAnchor(hash: string): string | null {
  if (hash.length < 2) return null;
  try {
    return decodeURIComponent(hash.slice(1));
  } catch {
    return hash.slice(1);
  }
}

/** Имя и папка по id: `Сеть/SSH` → { name: "SSH", folder: "Сеть" }. */
export function splitId(id: string): { name: string; folder: string } {
  const i = id.lastIndexOf("/");
  return i < 0 ? { name: id, folder: "" } : { name: id.slice(i + 1), folder: id.slice(0, i) };
}
