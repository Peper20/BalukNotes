# Sourced by the kit's scripts: where the project is and what its
# `.claude/pipeline.json` says (reference: pipeline/config.md). Reads the file
# with jq, or with python3 where there is no jq.
#
# PROJECT_ROOT is the git top level of the current directory (a linked worktree
# is its own root: the config is committed), or PIPELINE_PROJECT_ROOT.

KIT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
PROJECT_ROOT=${PIPELINE_PROJECT_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}

# config_get <dotted.key>: a string value of the config (empty when the file,
# the key or a reader is missing).
config_get() {
  local file=$PROJECT_ROOT/.claude/pipeline.json out
  [[ -f $file ]] || return 0
  if out=$(jq -r ".$1 // empty" "$file" 2>/dev/null); then
    printf '%s' "$out"
    return 0
  fi
  python3 -I -c '
import json, sys
v = json.load(open(sys.argv[1]))
for k in sys.argv[2].split("."):
    v = v.get(k) if isinstance(v, dict) else None
if isinstance(v, str):
    sys.stdout.write(v)
' "$file" "$1" 2>/dev/null
}

# config_tests: the rules of `tests`, one per line: the regex, a U+001F
# separator, the command.
config_tests() {
  local file=$PROJECT_ROOT/.claude/pipeline.json
  [[ -f $file ]] || return 0
  jq -r '.tests[]? | [.match, .run] | join("\u001f")' "$file" 2>/dev/null && return 0
  python3 -I -c '
import json, sys
for t in json.load(open(sys.argv[1])).get("tests", []):
    print(t["match"] + "\x1f" + t["run"])
' "$file" 2>/dev/null
}

# project_path <path>: a path of the config as an absolute one (`~/` is the
# home, a relative path is against the project root).
project_path() {
  case $1 in
    "~/"*) echo "$HOME/${1#"~/"}" ;;
    /*) echo "$1" ;;
    *) echo "$PROJECT_ROOT/$1" ;;
  esac
}
