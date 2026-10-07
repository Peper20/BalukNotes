#!/usr/bin/env bash
# Install BalukNotes for use from any folder - one version from this
# repository: the `notes` command with the parts `notes-typst` (release build:
# the baluk library, the client and fonts are embedded) and `notes-app` (the
# window, with a menu entry and an icon), the Claude Code skill /baluk-note.
# Run again after project changes.
#
#   tools/install.sh
#
# Where: notes and its parts - to $BIN (~/.local/bin by default), the menu entry
# and icon - to $XDG_DATA_HOME (~/.local/share), the skill - to
# $CLAUDE_SKILLS/baluk-note (~/.claude/skills by default). The vault is not
# touched: where it is - `notes info`.
set -euo pipefail
cd "$(dirname "$0")/.."

# One step lower than normal priority (cargo, npm, e2e inherit it): a build on
# all cores yields to work at the computer (nice 1; ionice - scale 0-7,
# normal 4) and takes idle CPU just the same.
renice -n 1 -p $$ >/dev/null 2>&1 || true
ionice -c 2 -n 5 -p $$ >/dev/null 2>&1 || true

bin=${BIN:-$HOME/.local/bin}
skills=${CLAUDE_SKILLS:-$HOME/.claude/skills}

echo "▶ client (app/dist)"
[[ -d app/node_modules ]] || npm --prefix app ci
npm --prefix app run -s build

echo "▶ notes, release build"
cargo build --release -p notes -p notes-typst -p notes-app

# Via a temporary file: a running `notes serve` does not block the replacement.
# The parts go to the same folder as notes (it looks for them there).
mkdir -p "$bin"
for name in notes notes-typst notes-app; do
  install -m755 "target/release/$name" "$bin/.$name.new"
  mv -f "$bin/.$name.new" "$bin/$name"
  echo "  $bin/$name"
done

# The window's menu entry and icon. The entry file name is the application id
# (tauri.conf.json): KDE and GNOME find the window icon by it.
app_id=$(sed -n 's/.*"identifier": "\(.*\)".*/\1/p' crates/notes-app/tauri.conf.json)
data=${XDG_DATA_HOME:-$HOME/.local/share}
install -Dm644 app/public/assets/icon.svg "$data/icons/hicolor/scalable/apps/$app_id.svg"
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

# The skill is replaced entirely by the copy from the repository (skills/baluk-note).
target=$skills/baluk-note
if [[ -e $target && ! -f $target/SKILL.md ]]; then
  echo "Error: $target exists but is not a skill (no SKILL.md) - leaving it alone." >&2
  exit 1
fi
mkdir -p "$skills"
rm -rf "$target.new"
cp -r skills/baluk-note "$target.new"
rm -rf "$target"
mv "$target.new" "$target"
echo "  $target"
# The former skill name (/new-note) goes to the trash, so Claude Code does not have two.
old=$skills/new-note
if [[ -f $old/SKILL.md ]]; then
  gio trash "$old" 2>/dev/null || rm -rf "$old"
  echo "  removed the former skill $old"
fi

case ":$PATH:" in
  *":$bin:"*) ;;
  *) echo "Warning: $bin is not in PATH - add it, otherwise there is no notes command." >&2 ;;
esac
# The autostart service (`notes service`, unit baluk-notes.service) moves to
# the new version; a `notes serve` started by hand is restarted by the user.
# A unit without `--socket`: the `notes app` window does not find the service
# core and starts a second one; a unit with default flags is rewritten, with own flags - a hint.
unit=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/baluk-notes.service
service_pid=0
if systemctl --user -q is-active baluk-notes.service 2>/dev/null; then
  if grep -q '^ExecStart=.* serve --addr 127.0.0.1:8421$' "$unit"; then
    "$bin/notes" service install >/dev/null 2>&1
    echo "  baluk-notes service: unit updated (socket for the window), restarted"
  else
    systemctl --user restart baluk-notes.service
    echo "  baluk-notes service restarted"
    grep -q -- '--socket' "$unit" ||
      echo "The service has no --socket: the window will start its own core. To update - notes service install with its flags." >&2
  fi
  service_pid=$(systemctl --user show -p MainPID --value baluk-notes.service)
fi
if pgrep -x notes-typst | grep -vqx "$service_pid"; then
  echo "An old version of notes serve is running - restart it."
fi
echo
"$bin/notes" info
