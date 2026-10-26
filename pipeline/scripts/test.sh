#!/usr/bin/env bash
# The targeted tests of a change (pipeline/docs/README.md, "Checks"): the
# commands that `tests` of .claude/pipeline.json maps the changed paths to.
#
#   pipeline/scripts/test.sh [--dry-run] [<path>...]
#
# Without paths: the files changed against origin/main, committed, uncommitted
# and new (main when there is no origin/main). `tests` is a list of rules
# { "match": "<regex>", "run": "<command>" }: a rule applies to every path its
# extended regex matches, and `$1`..`$9` in the command are the groups of the
# match. Each distinct command runs once, one after another, with the CPU cap
# of the checks (lib/cap.sh) and a lower priority; it takes the lock of heavy
# work itself where it needs it (the lock is not reentrant, so this script does
# not hold it). At the end a table of the commands; exit 1 if one failed.
# --dry-run only prints the commands.
set -uo pipefail

dry=0 paths=()
for arg in "$@"; do
  case $arg in
    --dry-run) dry=1 ;;
    -h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "test.sh: unknown option: $arg (known: --dry-run)" >&2; exit 2 ;;
    *) paths+=("$arg") ;;
  esac
done

renice -n 2 -p $$ >/dev/null 2>&1 || true
ionice -c 2 -n 5 -p $$ >/dev/null 2>&1 || true

kit=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
source "$kit/lib/config.sh"
cd "$PROJECT_ROOT" || exit 2
source "$kit/lib/cap.sh"
shopt -u patsub_replacement 2>/dev/null

rules=()
while IFS= read -r line; do
  [[ -n $line ]] && rules+=("$line")
done < <(config_tests)
if (( ${#rules[@]} == 0 )); then
  echo "test.sh: no \"tests\" in .claude/pipeline.json (or neither jq nor python3 can read it)" >&2
  exit 2
fi

if (( ${#paths[@]} == 0 )); then
  base=origin/main
  git rev-parse --verify -q "$base" >/dev/null || base=main
  if ! mb=$(git merge-base "$base" HEAD 2>/dev/null); then
    echo "test.sh: no $base to compare with; give the paths" >&2
    exit 2
  fi
  mapfile -t paths < <({ git diff --name-only "$mb"; git ls-files --others --exclude-standard; } | sort -u)
fi

# The commands in the order of the first path that asks for them.
commands=()
add_command() {
  local c
  for c in "${commands[@]+"${commands[@]}"}"; do
    [[ $c == "$1" ]] && return
  done
  commands+=("$1")
}
for path in "${paths[@]+"${paths[@]}"}"; do
  for rule in "${rules[@]}"; do
    regex=${rule%%$'\x1f'*} run=${rule#*$'\x1f'}
    [[ $path =~ $regex ]] || continue
    for i in 1 2 3 4 5 6 7 8 9; do
      value=${BASH_REMATCH[i]-}
      run=${run//\$$i/$(printf '%q' "$value")}
    done
    add_command "$run"
  done
done

if (( ${#commands[@]} == 0 )); then
  echo "no targeted tests for these changes (${#paths[@]} path(s))"
  exit 0
fi

if (( dry )); then
  printf '%s\n' "${commands[@]}"
  exit 0
fi

results=() times=()
failed=0
for cmd in "${commands[@]}"; do
  echo "▶ $cmd"
  start=$SECONDS
  bash -c "$cmd"
  rc=$?
  if (( rc == 0 )); then results+=("ok"); else results+=("failed ($rc)"); failed=1; fi
  times+=("$((SECONDS - start))")
done

echo
echo "result        time, s  command"
for i in "${!commands[@]}"; do
  printf '%-13s %-8s %s\n' "${results[$i]}" "${times[$i]}" "${commands[$i]}"
done
exit "$failed"
