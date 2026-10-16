#!/usr/bin/env bash
# Tests of the shell libraries of tools/lib that decide what happens to the
# build dir: the target limit (pruning, the size limit, the warnings), the build
# settings of a small disk and a cloud session, the WebKit check, and the lock's
# record of the checkout (the sources are freshened only after another
# checkout). No cargo build: cargo runs for `cargo metadata` and for `cargo
# clean` of a scratch dir. Run by check.sh as a step. The scratch dirs are in
# tests/.data/libs; the real build dir is never used. The tools that the
# libraries call (jq, df, python3, pkg-config) are replaced by shims in tests/.data/libs/.
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

echo
if (( fails == 0 )); then
  echo "libs tests passed"
else
  echo "$fails libs test(s) FAILED"
fi
exit "$fails"
