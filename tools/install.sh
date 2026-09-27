#!/usr/bin/env bash
# Поставить BalukNotes для работы из любой папки — одной версией из этого
# репозитория: команду `notes` (релизная сборка: библиотека baluk, клиент и
# шрифты встроены) и навык Claude Code /new-note. После изменений проекта —
# запустить снова.
#
#   tools/install.sh
#
# Куда: notes — в $BIN (по умолчанию ~/.local/bin), навык — в
# $CLAUDE_SKILLS/new-note (по умолчанию ~/.claude/skills). Хранилище не
# трогается: где оно — `notes info`.
set -euo pipefail
cd "$(dirname "$0")/.."

bin=${BIN:-$HOME/.local/bin}
skills=${CLAUDE_SKILLS:-$HOME/.claude/skills}

echo "▶ клиент (app/dist)"
[[ -d app/node_modules ]] || npm --prefix app ci
npm --prefix app run -s build

echo "▶ notes, релизная сборка"
cargo build --release -p notes-cli

# Через временный файл: работающий `notes serve` не мешает замене.
mkdir -p "$bin"
install -m755 target/release/notes "$bin/.notes.new"
mv -f "$bin/.notes.new" "$bin/notes"
echo "  $bin/notes"

# Навык целиком заменяется копией из репозитория (skills/new-note).
target=$skills/new-note
if [[ -e $target && ! -f $target/SKILL.md ]]; then
  echo "Ошибка: $target есть, но это не навык (нет SKILL.md) — не трогаю." >&2
  exit 1
fi
mkdir -p "$skills"
rm -rf "$target.new"
cp -r skills/new-note "$target.new"
rm -rf "$target"
mv "$target.new" "$target"
echo "  $target"

case ":$PATH:" in
  *":$bin:"*) ;;
  *) echo "Внимание: $bin нет в PATH — добавьте его, иначе команды notes не будет." >&2 ;;
esac
if pgrep -x notes >/dev/null; then
  echo "Работает notes serve старой версии — перезапустите его."
fi
echo
"$bin/notes" info
