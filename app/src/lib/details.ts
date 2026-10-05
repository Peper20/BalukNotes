// Open <details> ("Ответы" etc.) across a note rebuild: the key is the
// anchor of the nearest section above and the <summary> text (and the index
// among equal ones in the section), not the index in the whole note - a
// block added above does not open someone else's.

/** Keys of all <details> in `root` in document order. */
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

/** Keys of the open <details>. */
export function openDetails(root: ParentNode): Set<string> {
  const keys = detailsKeys(root);
  return new Set([...root.querySelectorAll("details")].flatMap((d, i) => (d.open ? [keys[i]!] : [])));
}

/** Opens the <details> with keys from `open`. */
export function restoreDetails(root: ParentNode, open: Set<string>): void {
  if (!open.size) return;
  const keys = detailsKeys(root);
  root.querySelectorAll("details").forEach((d, i) => open.has(keys[i]!) && (d.open = true));
}
