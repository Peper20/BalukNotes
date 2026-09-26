#!/usr/bin/env python3
"""Линт исходников заметок: то, что typst пропускает молча.

  1. Сырая десятичная запятая в матрежиме: `$0,5$` печатается «0, 5».
     Нужно `$dc("0,5")$` (см. baluk/math.typ).
  2. Конструкция `{,}` из LaTeX — в typst скобки выводятся буквально.
  3. smallcaps(...) — у шрифтов может не быть кириллических капителей, typst
     молча печатает строчные. Нужно small-caps(...) из библиотеки.

Запуск из корня проекта:  python3 baluk/lint.py [папка ...]
По умолчанию проверяет все .typ в проекте, кроме самой библиотеки.
Код возврата 1, если есть находки.
"""
import pathlib
import re
import sys

MATH = re.compile(r"\$[^$]*\$", re.S)
DECIMAL = re.compile(r"(?<![\w.])(\d+),(\d+)")

# Где запятая — законный разделитель: вырезаем до поиска дробей.
SEPARATORS = [
    re.compile(r'dc\("[^"]*"\)'),                          # уже обёрнутое
    re.compile(r"[NUE]\s*\(\s*[\d.]+\s*,\s*[\d.]+\s*\)"),  # N(0,1), U(0,1)
    re.compile(r"_\(\s*[^()]*,[^()]*\)"),                  # индексы x_(1,2)
    re.compile(r"\{[^{}]*\}"),                             # множества {1,2,3}
    re.compile(r"\\\{[^{}]*\\\}"),                         # \{2,3\}
    re.compile(r"\(\s*-?\d+\s*(,\s*-?\d+\s*){1,3}\)"),     # кортежи (3,6)
    re.compile(r"\[\s*-?\d+\s*,\s*-?\d+\s*\]"),            # отрезки [0,1]
    re.compile(r"\b[A-ZΦF]\s*\(\s*\d+\s*,\s*\d+\s*\)"),    # F(2,4)
]


def strip_comments(text):
    """Строки-комментарии typst выкидываем: в них живут примеры «как не надо»."""
    return "\n".join("" if line.lstrip().startswith("//") else line for line in text.split("\n"))


def check_file(path, root):
    text = strip_comments(path.read_text(encoding="utf-8"))
    rel = path.relative_to(root)
    found = []
    for m in MATH.finditer(text):
        cleaned = m.group(0)
        for sep in SEPARATORS:
            cleaned = sep.sub(lambda s: " " * len(s.group(0)), cleaned)
        for d in DECIMAL.finditer(cleaned):
            line = text.count("\n", 0, m.start() + d.start()) + 1
            found.append(f'{rel}:{line}: сырая запятая «{d.group(0)}» — нужно dc("{d.group(0)}")')
    for n, line in enumerate(text.split("\n"), 1):
        if "{,}" in line:
            found.append(f"{rel}:{n}: {{,}} из LaTeX — нужно dc(...)")
        if "smallcaps(" in line:
            found.append(f"{rel}:{n}: smallcaps() не работает с кириллицей — нужно small-caps(...)")
    return found


def main():
    root = pathlib.Path.cwd().resolve()
    dirs = [pathlib.Path(a).resolve() for a in sys.argv[1:]] or [root]
    files = []
    # Саму библиотеку не проверяем: там эти строки — в примерах.
    skip = lambda f: any(part in ("baluk", ".git") for part in f.relative_to(root).parts[:-1])
    for d in dirs:
        files += [f for f in d.rglob("*.typ") if not skip(f)]
    found = []
    for f in sorted(set(files)):
        found += check_file(f, root)
    for x in found:
        print(x)
    if found:
        print(f"\nнайдено: {len(found)}")
        return 1
    print(f"чисто ({len(files)} файлов)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
