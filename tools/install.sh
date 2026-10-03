#!/usr/bin/env bash
# Поставить BalukNotes для работы из любой папки — одной версией из этого
# репозитория: команду `notes` с частями `notes-typst` (релизная сборка:
# библиотека baluk, клиент и шрифты встроены) и `notes-app` (окно, с ярлыком
# в меню и значком), навык Claude Code /baluk-note. После изменений проекта —
# запустить снова.
#
#   tools/install.sh
#
# Куда: notes и части — в $BIN (по умолчанию ~/.local/bin), ярлык и значок — в
# $XDG_DATA_HOME (~/.local/share), навык — в
# $CLAUDE_SKILLS/baluk-note (по умолчанию ~/.claude/skills). Хранилище не
# трогается: где оно — `notes info`.
set -euo pipefail
cd "$(dirname "$0")/.."

# Приоритет на ступень ниже обычного (наследуют cargo, npm, e2e): сборка на
# всех ядрах уступает работе за компьютером (nice 1; ionice - шкала 0-7,
# обычный 4), а свободный процессор занимает так же.
renice -n 1 -p $$ >/dev/null 2>&1 || true
ionice -c 2 -n 5 -p $$ >/dev/null 2>&1 || true

bin=${BIN:-$HOME/.local/bin}
skills=${CLAUDE_SKILLS:-$HOME/.claude/skills}

echo "▶ клиент (app/dist)"
[[ -d app/node_modules ]] || npm --prefix app ci
npm --prefix app run -s build

echo "▶ notes, релизная сборка"
cargo build --release -p notes -p notes-typst -p notes-app

# Через временный файл: работающий `notes serve` не мешает замене. Части -
# в той же папке, что notes (там он их ищет).
mkdir -p "$bin"
for name in notes notes-typst notes-app; do
  install -m755 "target/release/$name" "$bin/.$name.new"
  mv -f "$bin/.$name.new" "$bin/$name"
  echo "  $bin/$name"
done

# Ярлык окна в меню и значок. Имя файла ярлыка - идентификатор приложения
# (tauri.conf.json): по нему KDE и GNOME находят значок окна.
app_id=$(sed -n 's/.*"identifier": "\(.*\)".*/\1/p' crates/notes-app/tauri.conf.json)
data=${XDG_DATA_HOME:-$HOME/.local/share}
install -Dm644 crates/notes-app/icons/icon.png "$data/icons/hicolor/256x256/apps/$app_id.png"
mkdir -p "$data/applications"
cat >"$data/applications/$app_id.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=baluk notes
Comment=Заметки на Typst
Exec=$bin/notes-app
Icon=$app_id
Terminal=false
Categories=Office;Education;
StartupWMClass=$app_id
DESKTOP
echo "  $data/applications/$app_id.desktop"

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
# Юнит без `--socket` - окно `notes app` не найдёт ядро службы и запустит
# второе: юнит с флагами по умолчанию переписывается, со своими - подсказка.
unit=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/baluk-notes.service
service_pid=0
if systemctl --user -q is-active baluk-notes.service 2>/dev/null; then
  if grep -q '^ExecStart=.* serve --addr 127.0.0.1:8421$' "$unit"; then
    "$bin/notes" service install >/dev/null 2>&1
    echo "  служба baluk-notes: юнит обновлён (сокет для окна), перезапущена"
  else
    systemctl --user restart baluk-notes.service
    echo "  служба baluk-notes перезапущена"
    grep -q -- '--socket' "$unit" ||
      echo "Служба без --socket: окно запустит своё ядро. Обновить - notes service install с её флагами." >&2
  fi
  service_pid=$(systemctl --user show -p MainPID --value baluk-notes.service)
fi
if pgrep -x notes-typst | grep -vqx "$service_pid"; then
  echo "Работает notes serve старой версии — перезапустите его."
fi
echo
"$bin/notes" info
