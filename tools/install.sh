#!/usr/bin/env bash
# Поставить BalukNotes для работы из любой папки — одной версией из этого
# репозитория: команду `notes` (релизная сборка: библиотека baluk, клиент и
# шрифты встроены) и навык Claude Code /baluk-note. После изменений проекта —
# запустить снова.
#
#   tools/install.sh
#
# Куда: notes — в $BIN (по умолчанию ~/.local/bin), навык — в
# $CLAUDE_SKILLS/baluk-note (по умолчанию ~/.claude/skills). Хранилище не
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

# Навык целиком заменяется копией из репозитория (skills/baluk-note).
target=$skills/baluk-note
if [[ -e $target && ! -f $target/SKILL.md ]]; then
  echo "Ошибка: $target есть, но это не навык (нет SKILL.md) — не трогаю." >&2
  exit 1
fi
mkdir -p "$skills"
rm -rf "$target.new"
cp -r skills/baluk-note "$target.new"
rm -rf "$target"
mv "$target.new" "$target"
echo "  $target"
# Прежнее имя навыка (/new-note) — в корзину, чтобы в Claude Code не было двух.
old=$skills/new-note
if [[ -f $old/SKILL.md ]]; then
  gio trash "$old" 2>/dev/null || rm -rf "$old"
  echo "  убран прежний навык $old"
fi

case ":$PATH:" in
  *":$bin:"*) ;;
  *) echo "Внимание: $bin нет в PATH — добавьте его, иначе команды notes не будет." >&2 ;;
esac
# Служба автозапуска (`notes service`, юнит baluk-notes.service) — на новую
# версию; запущенный вручную `notes serve` перезапускает пользователь.
service_pid=0
if systemctl --user -q is-active baluk-notes.service 2>/dev/null; then
  systemctl --user restart baluk-notes.service
  service_pid=$(systemctl --user show -p MainPID --value baluk-notes.service)
  echo "  служба baluk-notes перезапущена"
fi
if pgrep -x notes | grep -vqx "$service_pid"; then
  echo "Работает notes serve старой версии — перезапустите его."
fi
echo
"$bin/notes" info
