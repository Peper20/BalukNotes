// Renaming a note or a folder: what follows it in the client (tabs, reading
// places, collapsed folders). The server changes the files.

/** The new path of `id` after renaming `from` -> `to` (and everything inside `from`); untouched - null. */
export function movedId(id: string, from: string, to: string): string | null {
  if (id === from) return to;
  return id.startsWith(`${from}/`) ? to + id.slice(from.length) : null;
}
