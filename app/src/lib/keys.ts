// Shortcuts: "Ctrl+Shift+KeyF" -> an event check and the label
// "Ctrl+Shift+F". Letters and signs go by the physical key (event.code):
// shortcuts work in the Russian layout too. "?" goes by the character (it is
// in different places on different layouts).

export interface Combo {
  label: string;
  test: (e: KeyboardEvent) => boolean;
}

const NAMES: Record<string, string> = {
  BracketLeft: "[",
  BracketRight: "]",
  Equal: "=",
  Minus: "−",
  Comma: ",",
  Backslash: "\\",
  Slash: "/",
  Escape: "Esc",
  Enter: "Enter",
};

export function combo(spec: string): Combo {
  const parts = spec.split("+");
  const key = parts.pop()!;
  const mods = new Set(parts);
  const code = key.length === 1 ? null : key;
  const name = NAMES[key] ?? key.replace(/^Key|^Digit/, "");
  return {
    label: [...parts, name].join("+"),
    test: (e) =>
      (e.ctrlKey || e.metaKey) === mods.has("Ctrl") &&
      e.altKey === mods.has("Alt") &&
      (code ? e.shiftKey === mods.has("Shift") && e.code === code : e.key === key),
  };
}

/** Text input: single keys must not fire as commands. */
export const typing = (e: KeyboardEvent): boolean =>
  Boolean((e.target as Element | null)?.closest?.("input, select, textarea, [contenteditable]"));
