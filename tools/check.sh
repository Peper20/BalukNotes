#!/usr/bin/env bash
# Full project check in one command. Before a pull request: --fast; the full
# check runs on the head of every pull request (tools/check-pr.sh).
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
    -h|--help) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
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

source tools/lib/worktree-target.sh
# CHECK_JOBS: the CPU cap of the run, and the lock shared with other runs.
source tools/lib/cpu-cap.sh
# The build settings of a small disk and a cloud session (CARGO_PROFILE_DEV_DEBUG).
source tools/lib/build-env.sh

# The shared build dir grows without limit: pruning and the size limit
# (tools/lib/target-limit.sh) run first, under the lock of heavy work.
source tools/lib/target-limit.sh
heavy limit_target

logs=tests/.data/check
rm -rf "$logs"
mkdir -p "$logs"

names=() results=() times=()
failed=0

# step <name> <command...>: run it under the lock of heavy work (steps of all
# runs on the machine take turns: together they stay within CHECK_JOBS), record
# the result and time (the wait for the lock is not counted).
step() {
  local name="$1"; shift
  local log="$logs/$name.log" start
  echo "▶ $name: $*"
  heavy_acquire
  start=$SECONDS
  if "$@" >"$log" 2>&1 {HEAVY_FD}>&-; then
    results+=("ok")
  else
    results+=("failed")
    failed=1
    echo "  ✗ $name failed - tail of $log:"
    tail -n 25 "$log" | sed 's/^/    /'
  fi
  heavy_release
  names+=("$name")
  times+=("$((SECONDS - start))")
}

# skip <name> <reason>
skip() {
  names+=("$1") results+=("skipped: $2") times+=("-")
}

# notes check on tests/vault: exactly the intended problems. The expected total
# is the line "Total: `...`" in .claude/rules/tests-vault.md (the core test checks it too).
vault_check() {
  local expected actual
  expected=$(sed -n 's/^Total: `\(.*\)`$/\1/p' .claude/rules/tests-vault.md)
  if [[ -z $expected ]]; then
    echo "no line \"Total: \`...\`\" in .claude/rules/tests-vault.md"
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

# Copies of foreign crates with patches (.claude/rules/vendor.md) - their own tests,
# on one thread: the comemo cache is global (the rule says why).
vendor_tests() {
  CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$PWD/target}/vendor cargo test -q --manifest-path vendor/comemo/Cargo.toml --features testing -- --test-threads=1 || return 1
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
  # `vaults` is a light command: the thin notes runs it, notes-typst does not know it.
  cargo run -q -p notes -- --data "$dir" vaults new Проверка >/dev/null || return 1
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

# The core builds for the browser (docs/research/E4.md): notes-wasm and with it
# notes-core and notes-store for wasm32-unknown-unknown, lints included.
wasm_check() {
  if ! rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
    echo "no Rust target wasm32-unknown-unknown - install it: rustup target add wasm32-unknown-unknown"
    return 1
  fi
  cargo clippy -p notes-wasm --target wasm32-unknown-unknown -- -D warnings
}

# The pipeline mod (tools/pipeline, a Claude Code plugin; .claude/rules/tools.md,
# "Pipeline mod"): the manifest and the module, its tests, and the types with tsc.
# The types of the engine are laid beside the mod when Claude Code loads it
# (tools/pipeline/.claude-plugin/types, kept out of the repository), tsc is the
# client's: without either the type check is left out with a line in the log.
pipeline_mod() {
  claude plugin validate tools/pipeline || return 1
  claude plugin test tools/pipeline || return 1
  local tsc=app/node_modules/.bin/tsc
  [[ -x $tsc ]] || tsc=$(command -v tsc || true)
  if [[ -z $tsc ]]; then
    echo "no tsc (npm ci in app/): the types of the mod were not checked"
  elif [[ ! -f tools/pipeline/.claude-plugin/types/tsconfig.json ]]; then
    echo "the types of the engine are not laid (Claude Code lays them when it loads the mod): the types of the mod were not checked"
  else
    "$tsc" -p tools/pipeline || return 1
  fi
}

# The notes-app window (Tauri) - only with WebKitGTK; without it (cloud) - skipped.
no_webkit=()
if ! has_webkit; then
  no_webkit=(--exclude notes-app)
  echo "no webkit2gtk-4.1 - notes-app is not built"
fi

# The shell libraries of tools/lib (target limit, build settings, lock): no cargo build.
step tools-libs tools/test-libs.sh
if claude plugin test --help >/dev/null 2>&1; then
  step pipeline-mod pipeline_mod
else
  echo "no claude with plugin tests - the pipeline mod is not checked"
  skip pipeline-mod "no claude plugin"
fi

if [[ $rust == 1 ]]; then
  # Server tests serve the client from app/dist (a debug build reads it from
  # disk): a fresh checkout (a worktree) builds the client first.
  if [[ ! -f app/dist/index.html ]]; then
    [[ -d app/node_modules ]] || step npm-ci in_app npm ci
    step client-dist in_app npm run -s build
  fi
  step cargo-test cargo test --workspace "${no_webkit[@]}"
  # With the measure feature - also the heavy memory measurements (they do not run as tests).
  step clippy cargo clippy --workspace "${no_webkit[@]}" --all-targets --features notes-core/measure,notes-wasm/measure -- -D warnings
  step wasm wasm_check
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
      # e2e listens on fixed ports (8433, 8461, 8462): runs in parallel
      # worktrees must not overlap - the lock of the step takes care of it.
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
