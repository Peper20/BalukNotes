#!/usr/bin/env python3
"""Прототип сборщика (веха M0): хранилище .typ → статический сайт.

Потом эта логика переедет в notes-core (Rust). Что делает:
  1. кладёт свежую копию библиотеки konspekt/ в <хранилище>/_konspekt/;
  2. забирает цвета тем из _konspekt/css.typ и пишет themes.css;
  3. компилирует каждую заметку по разу на тему (typst --features html);
  4. склеивает: текст — из первой темы, у каждого рисунка div.k-frame —
     варианты всех тем; опорные цвета подсветки кода → CSS-переменные;
  5. пишет страницы, CSS и оглавление в каталог сборки.

  tools/build.py examples/vault build
"""

import html
import json
import re
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
LIB = REPO / "konspekt"
WEB = REPO / "web"

# Опорные цвета подсветки (konspekt/code.typ, опорные-цвета-кода) → переменные.
CODE_COLORS = {
    "#010100": "text", "#010101": "key", "#010102": "type", "#010103": "string",
    "#010104": "number", "#010105": "comment", "#010106": "function", "#010107": "hl",
}
FRAME_RE = re.compile(r'<div class="(k-frame[^"]*)">(<svg.*?</svg>)</div>', re.S)
BODY_RE = re.compile(r"<body>(.*)</body>", re.S)
HEAD_RE = re.compile(r"<head>(.*)</head>", re.S)


def typst(*args: str, cwd: Path) -> str:
    r = subprocess.run(["typst", *args], cwd=cwd, capture_output=True, text=True)
    noise = ("html export is under active development", "hint:", "deprecated")
    errors = "\n".join(l for l in r.stderr.splitlines() if l and not any(n in l for n in noise))
    if r.returncode != 0:
        raise RuntimeError(errors)
    return r.stdout


def sync_lib(vault: Path) -> None:
    dst = vault / "_konspekt"
    if dst.is_symlink():
        dst.unlink()
    if dst.exists():
        shutil.rmtree(dst)
    shutil.copytree(LIB, dst)


def load_themes(vault: Path) -> dict[str, dict[str, str]]:
    out = typst("eval", "--root", ".", "query(<k-css>).first().value",
                "--in", "_konspekt/css.typ", cwd=vault)
    return json.loads(out)


def themes_css(themes: dict[str, dict[str, str]]) -> str:
    parts = ["/* Сгенерировано tools/build.py из konspekt/theme.typ — не править. */"]
    for name, colors in themes.items():
        vars_ = "\n".join(f"  --k-{k}: {v};" for k, v in colors.items())
        parts.append(f':root[data-theme="{name}"] {{\n{vars_}\n}}')
        parts.append(f':root[data-theme="{name}"] .k-frame-v[data-theme="{name}"] {{ display: contents; }}')
    return "\n".join(parts) + "\n"


def notes(vault: Path) -> list[Path]:
    """Заметки — .typ вне служебных каталогов (_konspekt, code)."""
    return sorted(
        p.relative_to(vault) for p in vault.rglob("*.typ")
        if not any(part.startswith("_") or part == "code" for part in p.relative_to(vault).parts)
    )


def compile_note(vault: Path, note: Path, theme: str, tmp: Path) -> str:
    out = tmp / f"{note.with_suffix('').as_posix().replace('/', '__')}.{theme}.html"
    typst("compile", "--root", ".", "--features", "html", "--input", f"тема={theme}",
          note.as_posix(), str(out), cwd=vault)
    return out.read_text()


def merge(pages: dict[str, str]) -> tuple[str, str]:
    """Склейка компиляций по темам. Возвращает (head, body)."""
    names = list(pages)
    base = pages[names[0]]
    frames = {t: [svg for _, svg in FRAME_RE.findall(p)] for t, p in pages.items()}
    counts = {t: len(f) for t, f in frames.items()}
    if len(set(counts.values())) != 1:
        raise RuntimeError(f"разное число рисунков в темах: {counts}")

    it = iter(range(counts[names[0]]))

    def variants(m: re.Match) -> str:
        i = next(it)
        inner = "".join(
            f'<div class="k-frame-v" data-theme="{t}">{frames[t][i]}</div>' for t in names
        )
        return f'<div class="{m.group(1)}">{inner}</div>'

    body = FRAME_RE.sub(variants, BODY_RE.search(base).group(1))
    body = re.sub(r"#0101(0[0-7])", lambda m: f"var(--k-code-{CODE_COLORS['#0101' + m.group(1)]})", body)
    head = HEAD_RE.search(base).group(1)
    return head, body


THEME_SCRIPT = """<script>
(() => {
  const themes = %s;
  const root = document.documentElement;
  let saved = null;
  try { saved = localStorage.getItem("k-theme"); } catch {}
  const dark = matchMedia("(prefers-color-scheme: dark)").matches;
  root.dataset.theme = themes.includes(saved) ? saved : themes[dark && themes.length > 1 ? 1 : 0];
  addEventListener("DOMContentLoaded", () => {
    const b = document.getElementById("k-theme");
    const show = () => b.textContent = "тема: " + root.dataset.theme;
    show();
    b.onclick = () => {
      root.dataset.theme = themes[(themes.indexOf(root.dataset.theme) + 1) %% themes.length];
      try { localStorage.setItem("k-theme", root.dataset.theme); } catch {}
      show();
    };
  });
})();
</script>"""

TOOLBAR_CSS = """<style>
.k-toolbar { position: fixed; top: 0.6rem; right: 0.8rem; display: flex; gap: 0.5rem; font: 13px/1 var(--k-font-code); z-index: 10; }
.k-toolbar a, .k-toolbar button { color: var(--k-muted); background: var(--k-surface); border: 1px solid var(--k-line);
  border-radius: 4px; padding: 0.35rem 0.6rem; cursor: pointer; font: inherit; text-decoration: none; }
.k-toolbar a:hover, .k-toolbar button:hover { color: var(--k-accent); }
</style>"""


def page(head: str, body: str, depth: int, themes: list[str]) -> str:
    up = "../" * depth
    links = (f'<link rel="stylesheet" href="{up}konspekt.css">'
             f'<link rel="stylesheet" href="{up}themes.css">{TOOLBAR_CSS}'
             + THEME_SCRIPT % json.dumps(themes, ensure_ascii=False))
    toolbar = (f'<nav class="k-toolbar"><a href="{up}index.html">все заметки</a>'
               f'<button id="k-theme"></button></nav>')
    return (f'<!DOCTYPE html><html lang="ru"><head>{head}{links}</head>'
            f"<body>{toolbar}{body}</body></html>")


def main() -> None:
    vault, out = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
    sync_lib(vault)
    themes = load_themes(vault)
    names = list(themes)
    tmp = out / ".tmp"
    tmp.mkdir(parents=True, exist_ok=True)

    jobs = [(n, t) for n in notes(vault) for t in names]
    with ThreadPoolExecutor() as pool:
        results = list(pool.map(lambda j: _try(compile_note, vault, j[0], j[1], tmp), jobs))

    built, failed = [], []
    for note in notes(vault):
        pages = {t: r for (n, t), r in zip(jobs, results) if n == note}
        errs = [r for r in pages.values() if isinstance(r, Exception)]
        if errs:
            failed.append((note, errs[0]))
            continue
        try:
            head, body = merge(pages)
        except RuntimeError as e:
            failed.append((note, e))
            continue
        dst = out / note.with_suffix(".html")
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_text(page(head, body, len(note.parts) - 1, names))
        built.append(note)

    shutil.copy(WEB / "konspekt.css", out / "konspekt.css")
    (out / "themes.css").write_text(themes_css(themes))
    items = "".join(
        f'<li><a href="{html.escape(n.with_suffix(".html").as_posix())}">{html.escape(n.with_suffix("").as_posix())}</a></li>'
        for n in built
    )
    (out / "index.html").write_text(page(
        '<meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Заметки</title>',
        f'<header class="k-title"><h1>Заметки</h1></header><ul>{items}</ul>', 0, names))
    shutil.rmtree(tmp)

    print(f"собрано: {len(built)}, темы: {', '.join(names)} → {out}")
    for note, e in failed:
        print(f"ОШИБКА {note}:\n{e}", file=sys.stderr)
    sys.exit(1 if failed else 0)


def _try(f, *args):
    try:
        return f(*args)
    except Exception as e:  # noqa: BLE001 — ошибка компиляции одной заметки не валит сборку
        return e


if __name__ == "__main__":
    main()
