// Сочетания клавиш: «Ctrl+Shift+KeyF» → проверка события и подпись «Ctrl+Shift+F».
// Буквы и знаки — по физической клавише (event.code): сочетания работают и
// в русской раскладке. «?» — по символу (на разных раскладках он в разных местах).

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

/** Ввод текста: одиночные клавиши не должны срабатывать как команды. */
export const typing = (e: KeyboardEvent): boolean =>
  Boolean((e.target as Element | null)?.closest?.("input, select, textarea, [contenteditable]"));
