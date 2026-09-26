// Раскрытые <details> («Ответы» и т. п.) при пересборке заметки: ключ —
// якорь ближайшего раздела выше и текст <summary> (и порядковый номер среди
// одинаковых в разделе), а не номер по всей заметке — блок, добавленный
// выше, не раскроет чужой.

/** Ключи всех <details> в `root` по порядку документа. */
export function detailsKeys(root: ParentNode): string[] {
  const keys: string[] = [];
  const seen = new Map<string, number>();
  let section = "";
  for (const el of root.querySelectorAll(":is(h1, h2, h3, h4, h5, h6)[id], details")) {
    if (el.tagName !== "DETAILS") {
      section = el.id;
      continue;
    }
    const summary = [...el.children].find((c) => c.tagName === "SUMMARY")?.textContent?.trim() ?? "";
    const base = `${section}\n${summary}`;
    const n = seen.get(base) ?? 0;
    seen.set(base, n + 1);
    keys.push(`${base}\n${n}`);
  }
  return keys;
}

/** Ключи раскрытых <details>. */
export function openDetails(root: ParentNode): Set<string> {
  const keys = detailsKeys(root);
  return new Set([...root.querySelectorAll("details")].flatMap((d, i) => (d.open ? [keys[i]!] : [])));
}

/** Раскрыть <details> с ключами из `open`. */
export function restoreDetails(root: ParentNode, open: Set<string>): void {
  if (!open.size) return;
  const keys = detailsKeys(root);
  root.querySelectorAll("details").forEach((d, i) => open.has(keys[i]!) && (d.open = true));
}
