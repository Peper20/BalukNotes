#!/usr/bin/env bash
# Full project check in one command - before a commit and a PR.
#
#   tools/check.sh           # everything: Rust, the fixture vault, API types, client, e2e
#   tools/check.sh --fast    # without the client build and e2e (a minute or two)
#   tools/check.sh --rust    # Rust only: test, clippy, fmt, crate copies, notes check, note templates, API types
#   tools/check.sh --app     # client only: check, Vitest, build, e2e
#
# All steps run even if one fails (except dependent ones: e2e does not run
# without a client build). At the end - a table "step -> ok/failed, time";
# the output of a failed step is in tests/.data/check/<step>.log (its tail is printed).
# The exit code is non-zero if any step failed.
set -uo pipefail
cd "$(dirname "$0")/.."

rust=1 app=1 fast=0
for arg in "$@"; do
  case "$arg" in
    --fast) fast=1 ;;
    --rust) app=0 ;;
    --app) rust=0 ;;
    -h|--help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $arg (known: --fast, --rust, --app)" >&2; exit 2 ;;
  esac
done
if [[ $rust == 0 && $app == 0 ]]; then
  echo "--rust and --app together is just tools/check.sh" >&2
  exit 2
fi

# Lower than normal priority (cargo, npm, e2e inherit it): a build on all
# cores yields to work at the computer (nice 2; ionice - scale 0-7, normal 4)
# and takes idle CPU just the same.
renice -n 2 -p $$ >/dev/null 2>&1 || true
ionice -c 2 -n 5 -p $$ >/dev/null 2>&1 || true

# target/ grows without limit: cargo does not remove old variants of our crates
# (a new one appears after a Cargo.toml edit, a cache tag, another worktree path).
# Over TARGET_LIMIT_GB (30) - cargo clean first. Someone else's CARGO_TARGET_DIR
# is left alone: others (agents in worktrees) may be building in it right now.
limit_target() {
  local limit=${TARGET_LIMIT_GB:-30} size
  [[ -z ${CARGO_TARGET_DIR:-} && -d target ]] || return 0
  size=$(du -s --block-size=1G target | cut -f1)
  if (( size > limit )); then
    echo "target/ is $size GB, over the limit of $limit GB: cargo clean"
    cargo clean
  fi
}
limit_target

logs=tests/.data/check
rm -rf "$logs"
mkdir -p "$logs"

names=() results=() times=()
failed=0

# step <name> <command...>: run it, record the result and time.
step() {
  local name="$1"; shift
  local log="$logs/$name.log" start=$SECONDS
  echo "▶ $name: $*"
  if "$@" >"$log" 2>&1; then
    results+=("ok")
  else
    results+=("failed")
    failed=1
    echo "  ✗ $name failed - tail of $log:"
    tail -n 25 "$log" | sed 's/^/    /'
  fi
  names+=("$name")
  times+=("$((SECONDS - start))")
}

# skip <name> <reason>
skip() {
  names+=("$1") results+=("skipped: $2") times+=("-")
}

# notes check on tests/vault: exactly the intended problems. The expected total
# is the line "Итог: `...`" in .claude/rules/tests-vault.md (the core test checks it too).
vault_check() {
  local expected actual
  expected=$(sed -n 's/^Итог: `\(.*\)`$/\1/p' .claude/rules/tests-vault.md)
  if [[ -z $expected ]]; then
    echo "no line \"Итог: \`...\`\" in .claude/rules/tests-vault.md"
    return 1
  fi
  # Exit code 1 is expected: the fixture has intended errors.
  actual=$(cargo run -q -p notes-typst -- --data tests/.data --vault tests/vault check | tee /dev/stderr | tail -n 1)
  echo "expected: $expected"
  echo "actual:   $actual"
  [[ $actual == "$expected" ]]
}

# API types are exported from Rust and committed: no changes after the export.
api_types() {
  local dir=app/src/lib/api/types
  (cd app && npm run -s types) || return 1
  local changed
  changed=$(git status --porcelain -- "$dir")
  if [[ -n $changed ]]; then
    echo "API types are stale - export them (cd app && npm run types) and commit:"
    echo "$changed"
    git --no-pager diff --stat -- "$dir"
    return 1
  fi
}

# Copies of foreign crates with patches (.claude/rules/vendor.md) - their own tests.
vendor_tests() {
  CARGO_TARGET_DIR=target/vendor cargo test -q --manifest-path vendor/comemo/Cargo.toml --features testing || return 1
  rm -f vendor/comemo/Cargo.lock
}

# The /baluk-note skill: `notes new` templates (a note and a book; the file
# name comes from a title with markup characters and characters forbidden in
# file names), the examples skills/baluk-note/examples and the "Common calls"
# block of SKILL.md (appended to the note template) build in a temporary vault
# without errors, warnings or broken links.
baluk_note() {
  local dir=tests/.data/baluk-note notes=(cargo run -q -p notes-typst -- --data tests/.data/baluk-note)
  rm -rf "$dir"
  "${notes[@]}" vaults new Проверка >/dev/null || return 1
  notes+=(--vault Проверка)
  "${notes[@]}" new --tag проверка "Математика/Заметка" || return 1
  "${notes[@]}" new --book --lang en --title 'C++ [1] #x $y$ // - z' "Книга" || return 1
  "${notes[@]}" new --folder Сеть --title 'SSH: основы? "1.0"... <черновик> 1. -- a/b @x' || return 1
  [[ $("${notes[@]}" new --folder Сеть --title '/\:*?"<>|' | tail -n 1) == "Сеть/Без названия" ]] || return 1
  local vault=$dir/vaults/Проверка
  cp -r skills/baluk-note/examples "$vault/examples"
  awk '/^## Common calls/ {on = 1} on && /^```$/ {exit} on == 2 {print} on && /^```typst/ {on = 2}' \
    skills/baluk-note/SKILL.md >>"$vault/Математика/Заметка.typ"
  # the target of the #see link from the block
  mkdir -p "$vault/Math"
  printf '#import "/_baluk/lib.typ": *\n#show: note.with(title: [Производная])\n' >"$vault/Math/Derivative.typ"
  local actual
  actual=$("${notes[@]}" check | tee /dev/stderr | tail -n 1)
  [[ $actual == "notes: 10, errors: 0, warnings: 0, broken links: 0" ]]
}

in_app() { (cd app && "$@"); }

# The notes-app window (Tauri) - only with WebKitGTK; without it (cloud) - skipped.
no_webkit=()
if ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
  no_webkit=(--exclude notes-app)
  echo "no webkit2gtk-4.1 - notes-app is not built"
fi

if [[ $rust == 1 ]]; then
  step cargo-test cargo test --workspace "${no_webkit[@]}"
  # With the measure feature - also the heavy memory measurements (they do not run as tests).
  step clippy cargo clippy --workspace "${no_webkit[@]}" --all-targets --features notes-core/measure -- -D warnings
  step fmt cargo fmt --check
  step vendor vendor_tests
  step notes-check vault_check
  step baluk-note baluk_note
  step api-types api_types
fi

if [[ $app == 1 ]]; then
  if [[ ! -d app/node_modules ]]; then
    step npm-ci in_app npm ci
  fi
  step svelte-check in_app npm run -s check
  step vitest in_app npm test
  if [[ $fast == 1 ]]; then
    skip build "--fast"
    skip e2e "--fast"
  else
    step build in_app npm run -s build
    if [[ ${results[-1]} == "ok" ]]; then
      step e2e in_app npm run -s e2e
    else
      skip e2e "the client did not build"
    fi
  fi
fi

# printf pads by bytes, not by letters - pad ourselves.
cell() {
  local LC_ALL=C.UTF-8 s="$1" width="$2"
  printf '%s%*s' "$s" $((width - ${#s})) ''
}
echo
cell "step" 14; cell "result" 22; echo "time, s"
for i in "${!names[@]}"; do
  cell "${names[$i]}" 14; cell "${results[$i]}" 22; echo "${times[$i]}"
done
echo
if [[ $failed == 0 ]]; then
  echo "check passed (${SECONDS} s)"
else
  echo "CHECK FAILED (${SECONDS} s): logs in $logs/"
fi
exit "$failed"
