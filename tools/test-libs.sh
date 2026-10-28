#!/usr/bin/env bash
# Tests of the shell libraries of tools/lib that decide what happens to the
# build dir: the target limit (pruning, the size limit, the warnings), the build
# settings of a small disk and a cloud session, the WebKit check, and the lock's
# record of the checkout (the sources are freshened only after another
# checkout), tools/cargo.sh (its arguments, exit code, settings, record and the
# client build copied into a linked worktree), the shim tools/check-pr.sh,
# pipeline/setup.sh, and the settings of this project's tools from the CPU cap
# (tools/lib/cpu-env.sh). The kit's scripts
# (the runner, the cap and the lock, test.sh, measure.sh, check-pr.sh,
# ci-runner.sh, the cloud scripts, the copy of the mod's types) have their own
# tests, pipeline/scripts/test-scripts.sh, run at the end.
# No cargo build: cargo runs for `cargo metadata` and for `cargo clean` of a
# scratch dir; tools/cargo.sh runs in a scratch checkout against a cargo shim.
# Run by check.sh as a step. The scratch dirs are in tests/.data/libs; the real
# build dir is never used. The tools that the libraries call (jq, df, python3,
# pkg-config, cargo) are replaced by shims in tests/.data/libs/.
#
#   tools/test-libs.sh
set -uo pipefail
cd "$(dirname "$0")/.."
# check.sh exports the build settings of a small disk and a cloud session
# (build-env.sh): the tests set their own, so they do not inherit them.
unset CARGO_PROFILE_DEV_DEBUG CLAUDE_CODE_REMOTE BALUK_CLOUD

root=$PWD/tests/.data/libs
rm -rf "$root"
mkdir -p "$root/bin" "$root/no-python" "$root/no-webkit"

# Shims: jq must never be called (the libraries do not need it); df reports
# DF_TOTAL_GB and DF_FREE_GB (default: a big disk); python3 and pkg-config are
# missing or report no WebKit.
printf '#!/bin/sh\necho "jq must not be called" >&2\nexit 9\n' >"$root/bin/jq"
cat >"$root/bin/df" <<'SHIM'
#!/bin/sh
total=${DF_TOTAL_GB:-500}
free=${DF_FREE_GB:-400}
echo "Filesystem 1G-blocks Used Available Use% Mounted on"
echo "fake $total $((total - free)) $free 50% /"
SHIM
printf '#!/bin/sh\necho "python3: not here (test)" >&2\nexit 127\n' >"$root/no-python/python3"
printf '#!/bin/sh\nexit 1\n' >"$root/no-webkit/pkg-config"
chmod +x "$root/bin/jq" "$root/bin/df" "$root/no-python/python3" "$root/no-webkit/pkg-config"
export PATH="$root/bin:$PATH"

fails=0
expect() {   # expect <description> <command...>: the command must succeed
  local desc=$1
  shift
  if "$@"; then
    echo "  ok    $desc"
  else
    echo "  FAIL  $desc"
    fails=$((fails + 1))
  fi
}

# make_units <target dir>: a stale unit of a workspace crate (notes-core) and
# one of a dependency (serde, another hash), each with its fingerprint and rlib,
# 10 days old.
make_units() {
  local fp=$1/debug/.fingerprint
  mkdir -p "$fp/notes-core-0123456789abcdef" "$fp/serde-fedcba9876543210" "$1/debug/deps"
  : >"$fp/notes-core-0123456789abcdef/invoked.timestamp"
  : >"$fp/serde-fedcba9876543210/invoked.timestamp"
  touch -d '10 days ago' "$fp"/*/invoked.timestamp
  : >"$1/debug/deps/libnotes_core-0123456789abcdef.rlib"
  : >"$1/debug/deps/libserde-fedcba9876543210.rlib"
}

echo "workspace crates, without jq:"
expect "cargo metadata is read by python3 and lists notes-core" \
  eval '[[ $(source tools/lib/target-limit.sh; workspace_crates) == *notes-core* ]]'

echo "pruning, without jq:"
t=$root/target-prune
make_units "$t"
(source tools/lib/target-limit.sh; prune_target "$t") >"$root/prune.log" 2>&1
expect "a stale unit of a workspace crate is removed" test ! -e "$t/debug/.fingerprint/notes-core-0123456789abcdef"
expect "its rlib is removed" test ! -e "$t/debug/deps/libnotes_core-0123456789abcdef.rlib"
expect "a stale dependency stays" test -e "$t/debug/.fingerprint/serde-fedcba9876543210"
expect "its rlib stays" test -e "$t/debug/deps/libserde-fedcba9876543210.rlib"
expect "the log says what was removed" grep -q "prune: removed" "$root/prune.log"

echo "without python3 (no list of the crates):"
t=$root/target-nopy
make_units "$t"
(PATH="$root/no-python:$PATH"; source tools/lib/target-limit.sh; prune_target "$t") >"$root/nopy.log" 2>&1
rc=$?
expect "the pruning reports failure" test "$rc" -ne 0
expect "the pruning says it is skipped (WARNING, on stderr)" grep -q "WARNING: not pruned" "$root/nopy.log"
expect "nothing is removed" test -e "$t/debug/.fingerprint/notes-core-0123456789abcdef"
t2=$root/target-nopy-limit
mkdir -p "$t2/debug"
(PATH="$root/no-python:$PATH"; TARGET_LIMIT_GB=0; SHARED_TARGET=$t2; CARGO_TARGET_DIR=$t2
  source tools/lib/target-limit.sh; limit_target) >"$root/nopy-limit.log" 2>&1
expect "the size limit still applies without python3" grep -q "over the limit of 0 GB" "$root/nopy-limit.log"
expect "and the warning is printed" grep -q "WARNING: not pruned" "$root/nopy-limit.log"

echo "target limit by the free space:"
expect "a big disk: 30 GB" test "$(source tools/lib/target-limit.sh; target_limit_gb 400 22)" = 30
expect "a small disk: half of free plus size" test "$(source tools/lib/target-limit.sh; target_limit_gb 12 0)" = 6
expect "a small disk with a target: half of free plus size" test "$(source tools/lib/target-limit.sh; target_limit_gb 25 9)" = 17
t3=$root/target-limit
mkdir -p "$t3"
(export DF_TOTAL_GB=40 DF_FREE_GB=12; SHARED_TARGET=$t3; CARGO_TARGET_DIR=$t3; unset TARGET_LIMIT_GB
  source tools/lib/build-env.sh; source tools/lib/target-limit.sh; limit_target) >"$root/limit-small.log" 2>&1
expect "limit_target lowers the limit on a small disk (and says so)" grep -q "target limit: [0-9]* GB, below the default 30 GB: the disk has 12 GB free" "$root/limit-small.log"
(export DF_TOTAL_GB=500 DF_FREE_GB=400; SHARED_TARGET=$t3; CARGO_TARGET_DIR=$t3; unset TARGET_LIMIT_GB
  source tools/lib/build-env.sh; source tools/lib/target-limit.sh; limit_target) >"$root/limit-big.log" 2>&1
expect "limit_target keeps 30 GB on a big disk (no message)" test ! -s "$root/limit-big.log"
(export DF_TOTAL_GB=40 DF_FREE_GB=12; SHARED_TARGET=$t3; CARGO_TARGET_DIR=$t3; export TARGET_LIMIT_GB=7
  source tools/lib/build-env.sh; source tools/lib/target-limit.sh; limit_target) >"$root/limit-set.log" 2>&1
expect "TARGET_LIMIT_GB overrides the default (only the build line of build-env.sh)" \
  test "$(cat "$root/limit-set.log")" = "build: small disk, 40 GB - line tables only (CARGO_PROFILE_DEV_DEBUG)"

# build_profile <assignments...>: CARGO_PROFILE_DEV_DEBUG after build-env.sh in a
# clean environment with the assignments (none - "none").
build_profile() {
  (
    unset CARGO_PROFILE_DEV_DEBUG CLAUDE_CODE_REMOTE BALUK_CLOUD
    local a
    for a in "$@"; do export "$a"; done
    export CARGO_TARGET_DIR=$root/probe
    source tools/lib/build-env.sh 2>/dev/null
    echo "${CARGO_PROFILE_DEV_DEBUG:-none}"
  )
}
echo "build settings of a small disk and a cloud session:"
expect "a small disk (40 GB): line tables only" test "$(build_profile DF_TOTAL_GB=40)" = line-tables-only
expect "a big disk: no setting" test "$(build_profile DF_TOTAL_GB=500)" = none
expect "a cloud session (CLAUDE_CODE_REMOTE=true) on a big disk: line tables only" \
  test "$(build_profile CLAUDE_CODE_REMOTE=true DF_TOTAL_GB=500)" = line-tables-only
expect "BALUK_CLOUD=1: line tables only" test "$(build_profile BALUK_CLOUD=1 DF_TOTAL_GB=500)" = line-tables-only
expect "CLAUDE_CODE_REMOTE=false is not a cloud session" test "$(build_profile CLAUDE_CODE_REMOTE=false DF_TOTAL_GB=500)" = none
expect "a value set by hand is kept" test "$(build_profile CARGO_PROFILE_DEV_DEBUG=full DF_TOTAL_GB=40)" = full
(
  unset CARGO_PROFILE_DEV_DEBUG
  export CLAUDE_CODE_REMOTE=true CARGO_TARGET_DIR=$root/probe
  source tools/lib/build-env.sh
) >"$root/announce.log" 2>&1
expect "the setting is announced on stderr" grep -q "cloud session - line tables only" "$root/announce.log"
expect "without WebKitGTK (pkg-config reports nothing) has_webkit fails" \
  test "$(PATH="$root/no-webkit:$PATH"; source tools/lib/build-env.sh 2>/dev/null; has_webkit >/dev/null 2>&1; echo $?)" -ne 0

echo "the CPU cap (the formula and the lock are tested by pipeline/scripts/test-scripts.sh; here the settings of this project's tools, tools/lib/cpu-env.sh):"
# cap_values <cpus> [CHECK_JOBS]: CHECK_JOBS, the jobs of cargo, the test threads and
# the rayon threads that cpu-cap.sh sets on a machine with that many CPUs (an nproc shim),
# and cap_pw <cpus> <CHECK_JOBS>: the Playwright workers.
mkdir -p "$root/nproc-bin"
printf '#!/bin/sh\necho "$NPROC_SHIM"\n' >"$root/nproc-bin/nproc"
chmod +x "$root/nproc-bin/nproc"
cap_pw() {
  (
    export NPROC_SHIM=$1 PATH="$root/nproc-bin:$PATH" CHECK_JOBS=$2
    source tools/lib/cpu-cap.sh
    echo "$PLAYWRIGHT_WORKERS"
  )
}
cap_values() {
  (
    export NPROC_SHIM=$1 PATH="$root/nproc-bin:$PATH"
    unset CHECK_JOBS
    [[ -z ${2:-} ]] || export CHECK_JOBS=$2
    source tools/lib/cpu-cap.sh
    echo "$CHECK_JOBS $CARGO_BUILD_JOBS $RUST_TEST_THREADS $RAYON_NUM_THREADS"
  )
}
expect "12 CPUs, CHECK_JOBS unset: cap 10, cargo 9, test threads 9, rayon 8" test "$(cap_values 12)" = "10 9 9 8"
expect "an explicit CHECK_JOBS wins" test "$(cap_values 12 4)" = "4 3 3 2"
expect "an explicit CHECK_JOBS is capped at the CPUs of the machine" test "$(cap_values 6 20)" = "6 5 5 4"
expect "1 CPU: everything is 1" test "$(cap_values 1)" = "1 1 1 1"
expect "2 CPUs: the cap is 2, the rest at least 1" test "$(cap_values 2)" = "2 1 1 1"
expect "3 CPUs: cap 2" test "$(cap_values 3)" = "2 1 1 1"
expect "a cap of 1 gives 1 everywhere" test "$(cap_values 12 1)" = "1 1 1 1"
expect "Playwright workers: 1 at a cap of 1-7" \
  eval '[[ $(cap_pw 12 1) == 1 && $(cap_pw 12 4) == 1 && $(cap_pw 12 7) == 1 ]]'
expect "Playwright workers: 2 at a cap of 8-11" eval '[[ $(cap_pw 12 8) == 2 && $(cap_pw 12 11) == 2 ]]'
expect "Playwright workers: 3 at a cap of 12" test "$(cap_pw 16 12)" = 3
expect "Playwright workers: at most 8 from a cap of 32" \
  eval '[[ $(cap_pw 64 31) == 7 && $(cap_pw 64 32) == 8 && $(cap_pw 64 48) == 8 ]]'

echo "the lock's record of the checkout:"
(
  export HEAVY_LOCK=$root/heavy.lock
  source tools/lib/cpu-cap.sh
  freshen_sources() { echo freshen >>"$root/freshen.log"; }
  heavy true
  heavy true
  echo /some/other/checkout >"$HEAVY_LOCK.last"
  heavy true
  heavy true
)
expect "the sources are freshened after a start and after another checkout, not on a repeated run (2 times)" \
  test "$(wc -l <"$root/freshen.log")" -eq 2
expect "the record is the checkout's path" test "$(cat "$root/heavy.lock.last")" = "$(git rev-parse --show-toplevel)"

echo "tools/cargo.sh (in a scratch checkout of its own: the sources and the record of this one are not touched):"
cg=$root/cargo-checkout
mkdir -p "$cg/tools/lib" "$cg/pipeline/scripts" "$cg/.claude" "$root/cargo-bin"
cp tools/cargo.sh "$cg/tools/"
cp tools/lib/worktree-target.sh tools/lib/cpu-cap.sh tools/lib/cpu-env.sh tools/lib/build-env.sh "$cg/tools/lib/"
cp -r pipeline/scripts/lib "$cg/pipeline/scripts/"
cp .claude/pipeline.json "$cg/.claude/"
(cd "$cg" && git init -q && git add -A)
# The cargo of the tests: writes its arguments, one per line, to $CARGO_SHIM_OUT,
# CARGO_BUILD_JOBS to $CARGO_SHIM_OUT.jobs, and exits with $CARGO_SHIM_RC.
cat >"$root/cargo-bin/cargo" <<'SHIM'
#!/bin/sh
for a in "$@"; do printf '%s\n' "$a"; done >"$CARGO_SHIM_OUT"
printf '%s\n' "$CARGO_BUILD_JOBS" >"$CARGO_SHIM_OUT.jobs"
exit "${CARGO_SHIM_RC:-0}"
SHIM
chmod +x "$root/cargo-bin/cargo"
# run_cargo <arguments>: tools/cargo.sh in the scratch checkout, the shim first in
# PATH, CHECK_JOBS=3, a lock of its own; SHIM_RC sets the shim's exit code.
run_cargo() {
  (cd "$cg" && CHECK_JOBS=3 PATH="$root/cargo-bin:$PATH" HEAVY_LOCK="$root/cargo.lock" \
    CARGO_SHIM_OUT="$root/cargo.out" CARGO_SHIM_RC="${SHIM_RC:-0}" tools/cargo.sh "$@")
}

rm -f "$root/cargo.out" "$root/cargo.out.jobs" "$root/cargo.lock.last"
run_cargo test -p notes-core "with a space" '$HOME' '*'
printf '%s\n' test -p notes-core "with a space" '$HOME' '*' >"$root/cargo.expected"
expect "the arguments reach cargo verbatim (one with a space, a literal \$HOME and a *)" \
  cmp -s "$root/cargo.out" "$root/cargo.expected"
expect "CARGO_BUILD_JOBS is set" grep -Eq '^[1-9][0-9]*$' "$root/cargo.out.jobs"
expect "CARGO_BUILD_JOBS is the cap's setting for CHECK_JOBS=3" \
  test "$(cat "$root/cargo.out.jobs")" = "$(CHECK_JOBS=3; source tools/lib/cpu-cap.sh; echo "$CARGO_BUILD_JOBS")"
SHIM_RC=7 run_cargo build
rc=$?
expect "cargo's exit code is passed through" test "$rc" -eq 7
echo /some/other/checkout >"$root/cargo.lock.last"
run_cargo version
expect "the lock's record names the checkout after the run" \
  test "$(cat "$root/cargo.lock.last")" = "$(cd "$cg" && git rev-parse --show-toplevel)"
d=$(mktemp -d)
mkdir -p "$d/tools" && cp tools/cargo.sh "$d/tools/"
(cd "$d" && tools/cargo.sh --version) >"$root/outside.log" 2>&1
rc=$?
rm -rf "$d"
expect "outside a checkout of this repository it exits 2" test "$rc" -eq 2
expect "and says so" grep -q "not in a checkout of baluk notes" "$root/outside.log"


# absent <path>: nothing at the path, not even a dangling link.
absent() { [[ ! -e $1 && ! -L $1 ]]; }

echo "tools/cargo.sh test in a linked worktree: a copy of the main checkout's client build (scratch checkouts with git worktree):"
lk=$root/client-copy
rm -rf "$lk"
mkdir -p "$lk/main/tools/lib" "$lk/main/pipeline/scripts" "$lk/main/.claude" "$lk/main/app"
cp tools/cargo.sh "$lk/main/tools/"
cp tools/lib/worktree-target.sh tools/lib/cpu-cap.sh tools/lib/cpu-env.sh tools/lib/build-env.sh "$lk/main/tools/lib/"
cp -r pipeline/scripts/lib "$lk/main/pipeline/scripts/"
cp .claude/pipeline.json "$lk/main/.claude/"
cp .gitignore "$lk/main/"
echo '<!doctype html>' >"$lk/main/app/index.html"
(cd "$lk/main" && git init -q -b main && git add -A && git -c user.name=t -c user.email=t@t commit -q -m first)
# add_wt <dir>: a linked worktree of the main checkout, at its HEAD.
add_wt() { (cd "$lk/main" && git worktree add -q --detach "$1"); }
# run_client <checkout> <cargo arguments...>: tools/cargo.sh there, with the cargo
# shim (its arguments go to $root/client.out).
run_client() {
  local dir=$1
  shift
  (cd "$dir" && CHECK_JOBS=3 PATH="$root/cargo-bin:$PATH" HEAVY_LOCK="$root/client.lock" \
    CARGO_SHIM_OUT="$root/client.out" tools/cargo.sh "$@")
}

run_client "$lk/main" test -p notes-server >"$root/client-main.log" 2>&1
expect "the main checkout without a build: a test copies nothing and says nothing about the client" \
  eval 'absent "$lk/main/app/dist" && ! grep -q "^tools/cargo.sh: " "$root/client-main.log"'
add_wt "$lk/no-build"
run_client "$lk/no-build" test -p notes-server >"$root/client-no-build.log" 2>&1
expect "a linked worktree and no build in the main checkout: nothing is copied, and the build command is said" \
  eval 'absent "$lk/no-build/app/dist" && grep -qF "has no client build" "$root/client-no-build.log" && grep -qF "cd app && npm run build" "$root/client-no-build.log"'

mkdir -p "$lk/main/app/dist"
echo '<!doctype html>' >"$lk/main/app/dist/index.html"
add_wt "$lk/copied"
run_client "$lk/copied" version >"$root/client-version.log" 2>&1
expect "a command other than test copies nothing" absent "$lk/copied/app/dist"
run_client "$lk/copied" test -p notes-server >"$root/client-copied.log" 2>&1
expect "a test without app/dist copies the main checkout's build in: a directory of its own" \
  eval '[[ -d $lk/copied/app/dist && ! -L $lk/copied/app/dist ]] && cmp -s "$lk/copied/app/dist/index.html" "$lk/main/app/dist/index.html"'
expect "the copy is said in one line" \
  grep -qFx "tools/cargo.sh: app/dist is a copy of the main checkout's client build; a later change in app/ does not recopy it" "$root/client-copied.log"
expect "cargo still runs with the arguments" eval 'printf "%s\n" test -p notes-server | cmp -s - "$root/client.out"'
expect "git does not see the copy: the worktree is clean" eval '[[ -z $(cd "$lk/copied" && git status --porcelain) ]]'
# a client build empties app/dist first (vite)
rm -rf "$lk/copied/app/dist/"*
expect "a build in the worktree does not touch the main checkout's build" test -f "$lk/main/app/dist/index.html"
add_wt "$lk/own"
mkdir -p "$lk/own/app/dist"
echo own >"$lk/own/app/dist/index.html"
run_client "$lk/own" test -p notes-server >"$root/client-own.log" 2>&1
expect "its own app/dist is kept, and nothing is said" \
  eval '[[ ! -L $lk/own/app/dist && $(cat "$lk/own/app/dist/index.html") == own ]] && ! grep -q "^tools/cargo.sh: " "$root/client-own.log"'
add_wt "$lk/changed"
echo changed >>"$lk/changed/app/index.html"
run_client "$lk/changed" test -p notes-server >"$root/client-changed.log" 2>&1
expect "app/ differs from the main HEAD: nothing is copied, and the build command is said" \
  eval 'absent "$lk/changed/app/dist" && grep -qF "app/ differs from the main checkout" "$root/client-changed.log" && grep -qF "cd app && npm run build" "$root/client-changed.log"'
add_wt "$lk/added"
echo new >"$lk/added/app/new.ts"
run_client "$lk/added" test -p notes-server >"$root/client-added.log" 2>&1
expect "a new file in app/ counts as a difference too" \
  eval 'absent "$lk/added/app/dist" && grep -qF "app/ differs from the main checkout" "$root/client-added.log"'

echo "tools/check-pr.sh (the shim for the runner installed before the kit):"
expect "it hands its arguments to the kit's pipeline/scripts/check-pr.sh" \
  eval '[[ $(tools/check-pr.sh --help 2>&1 | head -n 2) == *"The full check of a pull request"* ]]'

# --- tools/web-bundle.sh: what it refuses before it builds anything ---
wb=$root/web-bundle
mkdir -p "$wb/foreign" "$wb/bundle/assets" "$wb/path"
echo keep >"$wb/foreign/file"
touch "$wb/bundle/index.html" "$wb/bundle/assets/config.js"
for tool in dirname ls; do ln -s "$(command -v "$tool")" "$wb/path/$tool"; done
run_wb() { "$BASH" tools/web-bundle.sh "$@" >"$root/wb.out" 2>&1; }
run_wb
rc=$?
expect "web-bundle without a folder: exit 2 and the usage" eval '[[ $rc -eq 2 ]] && grep -q "^usage: tools/web-bundle.sh <out-dir>" "$root/wb.out"'
run_wb "$wb/x" --nope
expect "web-bundle with an unknown option: exit 2" test $? -eq 2
run_wb "$wb/foreign"
rc=$?
expect "web-bundle refuses a folder that is not empty and not a bundle, and leaves it" \
  eval '[[ $rc -eq 1 ]] && grep -q "not empty and not a bundle" "$root/wb.out" && [[ $(cat "$wb/foreign/file") == keep ]]'
run_wb "$wb/x" --reuse-wasm
expect "web-bundle with --reuse-wasm and no folder: exit 2" test $? -eq 2
run_wb "$wb/x" --reuse-wasm "$wb/foreign"
rc=$?
expect "web-bundle --reuse-wasm names the missing file before it builds" \
  eval '[[ $rc -eq 1 ]] && grep -q "pkg/notes_wasm.js" "$root/wb.out"'
PATH=$wb/path run_wb "$wb/bundle"
rc=$?
expect "web-bundle accepts an earlier bundle, and names a missing tool before it builds" \
  eval '[[ $rc -eq 1 ]] && grep -q "node not found" "$root/wb.out" && [[ -f $wb/bundle/index.html ]]'

# --- pipeline/setup.sh: the links of the kit's agents and skills into a project ---
echo "pipeline/setup.sh (a scratch kit and a scratch project):"
su=$root/setup
rm -rf "$su"
mkdir -p "$su/kit/claude/agents" "$su/kit/claude/skills/s1" "$su/kit/claude/skills/s2" "$su/proj"
cp pipeline/setup.sh "$su/kit/setup.sh"
printf -- '---\nname: a1\n---\n' >"$su/kit/claude/agents/a1.md"
printf 'skill\n' >"$su/kit/claude/skills/s1/SKILL.md"
printf 'skill\n' >"$su/kit/claude/skills/s2/SKILL.md"
"$su/kit/setup.sh" "$su/proj" >"$su/setup.out" 2>&1
rc=$?
expect "the kit's agents and skills are linked into .claude of the project, by relative links that resolve" \
  eval '[[ $rc -eq 0 && $(readlink "$su/proj/.claude/agents/a1.md") == ../../../kit/claude/agents/a1.md && $(readlink "$su/proj/.claude/skills/s1") == ../../../kit/claude/skills/s1 && -f $su/proj/.claude/skills/s2/SKILL.md && -f $su/proj/.claude/agents/a1.md ]]'
expect "it says what it linked and that the config is missing" \
  eval 'grep -q "linked .claude/agents/a1.md" "$su/setup.out" && grep -q "pipeline.json is missing" "$su/setup.out"'
"$su/kit/setup.sh" "$su/proj" >"$su/setup.out" 2>&1
rc=$?
expect "a second run links nothing" \
  eval '[[ $rc -eq 0 ]] && ! grep -q "^linked" "$su/setup.out"'
rm "$su/proj/.claude/skills/s2" "$su/proj/.claude/agents/a1.md"
printf 'mine\n' >"$su/proj/.claude/agents/a1.md"
"$su/kit/setup.sh" "$su/proj" >"$su/setup.out" 2>&1
rc=$?
expect "a plain file in the place of a link is refused, and nothing else is linked" \
  eval '[[ $rc -eq 1 && $(cat "$su/proj/.claude/agents/a1.md") == mine && ! -e $su/proj/.claude/skills/s2 ]] && grep -q "is not a link" "$su/setup.out"'
rm "$su/proj/.claude/agents/a1.md"
ln -s ../../elsewhere "$su/proj/.claude/agents/a1.md"
"$su/kit/setup.sh" "$su/proj" >"$su/setup.out" 2>&1
rc=$?
expect "a link to something else is refused too" \
  eval '[[ $rc -eq 1 && $(readlink "$su/proj/.claude/agents/a1.md") == ../../elsewhere ]] && grep -q "not to the kit" "$su/setup.out"'

# --- tools/lib/cargo-test.sh: the test binaries run at the same time ---
echo "tools/lib/cargo-test.sh (fake test binaries, a cargo shim):"
ct=$root/cargo-test
rm -rf "$ct"
mkdir -p "$ct/bin" "$ct/crates"
# fake_test <kind> <package> <name> <tests> <seconds> <exit code> [failing test]: a
# test binary that lists <tests> tests, writes where it runs and with what to
# $ct/runs, and marks its start and end in $ct/events (time in ms, threads).
# Its line of the build's JSON goes to $ct/build.json; the id of the package
# `beta` is in the old form of cargo.
fake_test() {
  local kind=$1 pkg=$2 name=$3 tests=$4 secs=$5 rc=$6 failing=${7:-} id
  mkdir -p "$ct/crates/$pkg"
  printf '[package]\nname = "%s"\n' "$pkg" >"$ct/crates/$pkg/Cargo.toml"
  cat >"$ct/bin/$name" <<BIN
#!/bin/bash
if [[ \${1:-} == --list ]]; then
  for ((i = 1; i <= $tests; i++)); do echo "case_\$i: test"; done
  echo "a_bench: benchmark"
  exit 0
fi
now() { echo \$((\$(date +%s%N) / 1000000)); }
echo "$name \$PWD \$CARGO_MANIFEST_DIR \$CARGO_PKG_NAME \$CARGO_PKG_VERSION" >>"$ct/runs"
echo "start \$(now) \$RUST_TEST_THREADS" >>"$ct/events"
ps -o ni= -p \$\$ | tr -d ' ' >>"$ct/nice"
sleep $secs
echo "end \$(now) \$RUST_TEST_THREADS" >>"$ct/events"
[[ -z "$failing" ]] || echo "test $failing ... FAILED"
echo "test result: done"
exit $rc
BIN
  chmod +x "$ct/bin/$name"
  id="path+file://$ct/crates/$pkg#0.1.0"
  [[ $pkg == beta ]] && id="beta 0.1.0 (path+file://$ct/crates/$pkg)"
  printf '{"reason":"compiler-artifact","package_id":"%s","manifest_path":"%s","target":{"kind":["%s"],"name":"%s"},"profile":{"test":true},"executable":"%s"}\n' \
    "$id" "$ct/crates/$pkg/Cargo.toml" "$kind" "$name" "$ct/bin/$name" >>"$ct/build.json"
}
# The start of the build's JSON: a line that is not JSON, a message that is not
# an artifact and a build that is not a test stay out of the list.
ct_json_start() {
  printf 'not json\n{"reason":"build-finished","success":true}\n' >"$ct/build.json"
  printf '{"reason":"compiler-artifact","package_id":"x","manifest_path":"/x/Cargo.toml","target":{"kind":["lib"],"name":"x"},"profile":{"test":false},"executable":null}\n' >>"$ct/build.json"
}
# cargo: --no-run prints the JSON (or fails with CT_BUILD_RC), --doc prints the
# threads it was given and exits with CT_DOC_RC.
cat >"$ct/bin/cargo" <<SHIM
#!/bin/sh
case " \$* " in
  *" --no-run "*)
    echo "cargo shim: \$*" >>"$ct/cargo.args"
    if [ -n "\${CT_BUILD_RC:-}" ]; then echo "error: the build broke" >&2; exit "\$CT_BUILD_RC"; fi
    cat "$ct/build.json"
    echo "    Finished test profile" >&2 ;;
  *" --doc "*)
    echo "doc tests run with \$RUST_TEST_THREADS thread(s)"
    exit "\${CT_DOC_RC:-0}" ;;
esac
SHIM
chmod +x "$ct/bin/cargo"
# run_ct <budget> <cargo arguments...>: cargo_test with the shim, the output in
# $ct/out.log; ct_rc is its exit code.
run_ct() {
  local budget=$1
  shift
  (PATH="$ct/bin:$PATH"; RUST_TEST_THREADS=$budget; CARGO_TEST_TIMES=$ct/times
    source tools/lib/cargo-test.sh; cargo_test "$ct/work" "$@") >"$ct/out.log" 2>&1
  ct_rc=$?
}
# ct_threads_of <rank> <tests> <budget>
ct_threads_of() { (source tools/lib/cargo-test.sh; cargo_test_threads "$@"); }

expect "the longest binary gets half of the budget, rounded up (9 -> 5)" test "$(ct_threads_of 0 100 9)" = 5
expect "the others get a quarter (8 -> 2), rounded up (9 -> 3)" test "$(ct_threads_of 1 100 8)$(ct_threads_of 3 100 9)" = 23
expect "not more threads than tests" test "$(ct_threads_of 0 2 9)" = 2
expect "a binary of 0 tests gets 1" test "$(ct_threads_of 0 0 9)" = 1
expect "never more than the budget" test "$(ct_threads_of 0 50 1)$(ct_threads_of 1 50 1)$(ct_threads_of 0 50 3)" = 112

ct_json_start
fake_test lib gamma gamma_lib 3 0.3 0
fake_test test alpha it 6 0.5 0
fake_test bin beta beta_bin 2 0.3 0
fake_test lib alpha alpha_lib 6 0.5 0
run_ct 6 --workspace --exclude notes-app
expect "all binaries pass: the step passes" test "$ct_rc" -eq 0
expect "cargo got the arguments and --no-run" grep -q 'cargo shim: test --workspace --exclude notes-app --no-run' "$ct/cargo.args"
expect "every binary ran (4), and the doc tests" \
  eval '[[ $(wc -l <"$ct/runs") -eq 4 ]] && grep -q "doc tests run with 1 thread" "$ct/out.log"'
expect "the logs follow the order of the labels, the doc tests last" \
  eval '[[ $(grep "^=====" "$ct/out.log" | grep -v summary | sed "s/ (.*//") == $(printf "%s\n" "===== alpha lib" "===== alpha test it" "===== beta bin beta_bin" "===== gamma lib" "===== doc tests") ]]'
expect "a binary runs in the directory of its package with the variables of the package (also the old form of the id)" \
  eval '[[ $(grep "^alpha_lib " "$ct/runs" | cut -d" " -f2-) == "$ct/crates/alpha $ct/crates/alpha alpha 0.1.0" && $(grep "^beta_bin " "$ct/runs" | cut -d" " -f2-) == "$ct/crates/beta $ct/crates/beta beta 0.1.0" ]]'
expect "the summary lists a binary with its tests, as ok" \
  eval 'grep -Eq "^gamma lib +3 +[0-9]+ +[0-9]+ +ok$" "$ct/out.log" && ! grep -q FAILED "$ct/out.log"'
expect "jq was not needed" eval '! grep -q "jq must not be called" "$ct/out.log"'
# The threads of the binaries running at once (from the start and end marks).
ct_peak() {
  python3 -I - "$ct/events" <<'PY'
import sys
events = []
for line in open(sys.argv[1]):
    kind, ms, threads = line.split()
    events.append((int(ms), kind == "start", int(threads)))
events.sort(key=lambda e: (e[0], e[1]))  # an end before a start of the same millisecond
threads_now = binaries_now = most_threads = most_binaries = 0
for _, start, threads in events:
    sign = 1 if start else -1
    threads_now += sign * threads
    binaries_now += sign
    most_threads = max(most_threads, threads_now)
    most_binaries = max(most_binaries, binaries_now)
print(most_threads, most_binaries)
PY
}
read -r ct_threads ct_binaries < <(ct_peak)
expect "the test threads running at once stay within the budget (6)" test "$ct_threads" -le 6
expect "and binaries ran at the same time" test "$ct_binaries" -ge 2

# The priority of the step reaches the binaries (the runner lowers its own): they
# have the nice of the step, whatever it is (a shell already above 3 keeps its own).
rm -f "$ct/nice" "$ct/events"
(renice -n 3 -p "$BASHPID" >/dev/null 2>&1; ps -o ni= -p "$BASHPID" | tr -d ' ' >"$ct/own-nice"; run_ct 4 --workspace)
expect "the binaries inherit the priority of the step" \
  eval '[[ $(wc -l <"$ct/nice") -eq 4 && $(sort -u "$ct/nice") == $(cat "$ct/own-nice") ]]'
# A cap of 1-2 gives a budget of 1: the binaries run in turn.
rm -f "$ct/events"
run_ct 1 --workspace
read -r ct_threads ct_binaries < <(ct_peak)
expect "a budget of 1 thread: the binaries run in turn" eval '[[ $ct_rc -eq 0 && $ct_threads -eq 1 && $ct_binaries -eq 1 ]]'
rm -f "$ct/events"
run_ct 2 --workspace
read -r ct_threads ct_binaries < <(ct_peak)
expect "a budget of 2: at most 2 threads at once" test "$ct_threads" -le 2

ct_json_start
fake_test lib alpha alpha_lib 6 0.2 0
fake_test test alpha it 6 0.2 101 "api::signs_in"
fake_test lib gamma gamma_lib 3 0.2 0
run_ct 4 --workspace
expect "a failing binary: the step fails" test "$ct_rc" -eq 1
expect "the log names the failed binary and its failed test at the end" \
  eval '[[ $(tail -n 3 "$ct/out.log") == $(printf "%s\n" "FAILED test binaries:" "  alpha test it (exit 101)" "    api::signs_in") ]]'
expect "the others are ok in the summary" eval 'grep -Eq "^gamma lib +3 +[0-9]+ +[0-9]+ +ok$" "$ct/out.log"'
CT_DOC_RC=1 run_ct 4 --workspace
expect "failed doc tests fail the step and are named" \
  eval '[[ $ct_rc -eq 1 ]] && tail -n 3 "$ct/out.log" | grep -q "doc tests (exit 1)"'
CT_BUILD_RC=101 run_ct 4 --workspace
expect "a failing build: the step fails with the message of the build" \
  eval '[[ $ct_rc -eq 1 ]] && grep -q "error: the build broke" "$ct/out.log"'
ct_json_start
run_ct 4 --workspace
expect "a build without test binaries fails" eval '[[ $ct_rc -eq 1 ]] && grep -q "no test binaries" "$ct/out.log"'

# The order of starting: the threads of a binary in the summary say its rank
# (budget 9: the first gets 5, the others 3).
# ct_threads_in_summary <label>: the threads column of that binary.
ct_threads_in_summary() {
  sed -n '/^===== summary/,$p' "$ct/out.log" | grep -E "^$1 +[0-9]" | awk '{print $(NF - 2)}'
}
ct_json_start
fake_test lib alpha alpha_lib 8 0.2 0
fake_test test alpha it 8 0.2 0
fake_test lib gamma gamma_lib 9 0.2 0
rm -f "$ct/times"
run_ct 9 --workspace
expect "no seconds of a last run: the binary with the most tests goes first" \
  eval '[[ $ct_rc -eq 0 && $(ct_threads_in_summary "gamma lib") == 5 && $(ct_threads_in_summary "alpha lib") == 3 ]]'
expect "the seconds of every binary that passed are kept for the next run" \
  eval '[[ $(cut -f1 "$ct/times" | tr "\n" ,) == "alpha lib,alpha test it,doc tests,gamma lib," ]] && ! grep -qvP "\t[0-9]+$" "$ct/times"'
printf 'alpha test it\t30\ngamma lib\t5\nalpha lib\t4\ngone lib\t7\n' >"$ct/times"
run_ct 9 --workspace
expect "the longest binary of the last run goes first" \
  eval '[[ $(ct_threads_in_summary "alpha test it") == 5 && $(ct_threads_in_summary "gamma lib") == 3 ]]'
expect "the seconds of a binary no longer built stay" grep -qP '^gone lib\t7$' "$ct/times"
printf 'alpha test it\t30\ngamma lib\t5\nalpha lib\t4\n' >"$ct/times"
ct_json_start
fake_test lib alpha alpha_lib 8 0.2 0
fake_test test alpha it 8 0.2 101 "api::signs_in"
fake_test lib gamma gamma_lib 9 0.2 0
run_ct 9 --workspace
expect "a failed binary keeps its seconds of the last run, the others are updated" \
  eval '[[ $ct_rc -eq 1 ]] && grep -qP "^alpha test it\t30$" "$ct/times" && ! grep -qP "^gamma lib\t5$" "$ct/times"'
expect "a times file that cannot be written is no failure" \
  eval 'ct_json_start; fake_test lib gamma gamma_lib 9 0.1 0;
    (PATH="$ct/bin:$PATH"; RUST_TEST_THREADS=4; CARGO_TEST_TIMES=/proc/no-such/times
      source tools/lib/cargo-test.sh; cargo_test "$ct/work" --workspace) >"$ct/out.log" 2>&1'

# --- pipeline/scripts/test-scripts.sh: the kit's scripts ---
echo "pipeline/scripts/test-scripts.sh (the kit's scripts):"
pipeline/scripts/test-scripts.sh
fails=$((fails + $?))

echo
if (( fails == 0 )); then
  echo "libs tests passed"
else
  echo "$fails libs test(s) FAILED"
fi
exit "$fails"
