#!/usr/bin/env bash
# Tests of the shell libraries of tools/lib that decide what happens to the
# build dir: the target limit (pruning, the size limit, the warnings), the build
# settings of a small disk and a cloud session, the WebKit check, and the lock's
# record of the checkout (the sources are freshened only after another
# checkout), tools/cargo.sh (its arguments, exit code, settings and record) and
# tools/check-pr.sh (its arguments, comment text, exit codes, worktree) and
# tools/ci-runner.sh (its arguments, the generated config and unit, install and
# uninstall in scratch places).
# No cargo build: cargo runs for `cargo metadata` and for `cargo clean` of a
# scratch dir; tools/cargo.sh and tools/check-pr.sh run in scratch checkouts
# against a cargo, a tea and a check.sh shim; tools/ci-runner.sh against a tea
# and a systemctl shim.
# Run by check.sh as a step. The scratch dirs are in tests/.data/libs; the real
# build dir is never used. The tools that the libraries call (jq, df, python3,
# pkg-config, cargo, tea) are replaced by shims in tests/.data/libs/.
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

echo "tools/cargo.sh (in a scratch checkout of its own: the sources and the record of this one are not touched):"
cg=$root/cargo-checkout
mkdir -p "$cg/tools/lib" "$root/cargo-bin"
cp tools/cargo.sh "$cg/tools/"
cp tools/lib/worktree-target.sh tools/lib/cpu-cap.sh tools/lib/build-env.sh "$cg/tools/lib/"
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


echo "tools/check-pr.sh, the pieces (the output of check.sh is a sample):"
cat >"$root/sample-failed.out" <<'SAMPLE'
▶ cargo-test: cargo test --workspace
  ✗ cargo-test failed - tail of tests/.data/check/cargo-test.log:
    step    this line is indented, it is not the table

step          result                time, s
tools-libs    ok                    1
cargo-test    failed                42
fmt           ok                    3
build         skipped: --fast       -
e2e           failed                9

CHECK FAILED (55 s): logs in tests/.data/check/
SAMPLE
sed -e 's/failed  /ok      /' -e '/CHECK FAILED/d' "$root/sample-failed.out" >"$root/sample-passed.out"
echo "check passed (55 s)" >>"$root/sample-passed.out"
sha_a=$(printf 'a%.0s' {1..40})
sha_b=$(printf 'b%.0s' {1..40})
# fn <function> <args...>: a function of check-pr.sh, sourced in a subshell.
fn() { (source tools/check-pr.sh; "$@"); }
expect "the table of steps is cut from the first header to the blank line" \
  test "$(fn step_table "$root/sample-failed.out" | sed -n '1p;$p')" = "$(printf '%s\n%s' 'step          result                time, s' 'e2e           failed                9')"
expect "the failed steps are named in order, a skipped step is not failed" \
  test "$(fn failed_steps "$root/sample-failed.out" | paste -sd,)" = "cargo-test,e2e"
expect "a passed output has no failed steps" test -z "$(fn failed_steps "$root/sample-passed.out")"
expect "the first line of a pass" test "$(fn first_line "$sha_a" '')" = "Full check passed at $sha_a."
expect "the first line of a failure" \
  test "$(fn first_line "$sha_a" 'cargo-test, e2e')" = "Full check failed at $sha_a: cargo-test, e2e."
fn step_table "$root/sample-failed.out" >"$root/table.txt"
fn comment_text "$sha_a" 'cargo-test, e2e' "$root/table.txt" >"$root/comment-failed.txt"
expect "the comment of a failure: the line, a blank line, the table in a code block" \
  test "$(cat "$root/comment-failed.txt")" = "$(printf 'Full check failed at %s: cargo-test, e2e.\n\n```\n%s\n```' "$sha_a" "$(cat "$root/table.txt")")"
fn comment_text "$sha_a" '' "$root/table.txt" >"$root/comment-passed.txt"
expect "the comment of a pass starts with the exact line" test "$(head -n 1 "$root/comment-passed.txt")" = "Full check passed at $sha_a."
fn comment_text "$sha_a" '' "$root/table.txt" "$sha_b" >"$root/comment-moved.txt"
expect "a moved head: the first line is the same, one more line names both commits" \
  test "$(sed -n '1p;3p' "$root/comment-moved.txt")" = "$(printf 'Full check passed at %s.\nThe head of the pull request moved during the run to %s; this result is for %s.' "$sha_a" "$sha_b" "$sha_a")"
expect "a head that did not move adds no line" test "$(grep -c moved "$root/comment-passed.txt")" -eq 0

echo "tools/check-pr.sh (in a scratch checkout with its own origin, against tea and check.sh shims):"
cp_main=$root/pr-main cp_origin=$root/pr-origin.git cp_bin=$root/pr-bin
mkdir -p "$cp_main/tools/lib" "$cp_bin"
cp tools/check-pr.sh "$cp_main/tools/"
cp tools/lib/cpu-cap.sh "$cp_main/tools/lib/"
# The check.sh of the pull request: FAKE_CHECK=pass|fail; it prints a table of
# steps like the real one, writes the log of a failed step and changes a
# tracked file as a failed api-types step does.
cat >"$cp_main/tools/check.sh" <<'SHIM'
#!/usr/bin/env bash
rm -rf tests/.data/check && mkdir -p tests/.data/check
echo "▶ fake step"
if [[ ${FAKE_CHECK:-pass} == fail ]]; then
  echo "boom" >tests/.data/check/cargo-test.log
  echo "stale" >app-types.txt
  printf 'step          result                time, s\ncargo-test    failed                4\nbuild         skipped: --fast       -\n\nCHECK FAILED (4 s): logs in tests/.data/check/\n'
  exit 1
fi
printf 'step          result                time, s\ncargo-test    ok                    4\n\ncheck passed (4 s)\n'
SHIM
chmod +x "$cp_main/tools/check.sh"
echo fresh >"$cp_main/app-types.txt"
git init -q -b main "$cp_main"
git -C "$cp_main" add -A
git -C "$cp_main" -c user.name=t -c user.email=t@t commit -q -m first
git init -q --bare "$cp_origin"
git -C "$cp_main" remote add origin "$cp_origin"
head1=$(git -C "$cp_main" rev-parse HEAD)
git -C "$cp_origin" fetch -q "$cp_main" main
git -C "$cp_origin" update-ref refs/pull/7/head "$head1"
# tea: `api` prints $TEA_PR_JSON (from the second call on $TEA_PR_JSON2, if the
# file is there), `comment` writes its body to $TEA_OUT; failures by TEA_API_FAIL, TEA_COMMENT_RC.
cat >"$cp_bin/tea" <<'SHIM'
#!/bin/sh
case "$1" in
  api)
    [ -z "$TEA_API_FAIL" ] || { echo "tea: no server (test)" >&2; exit 1; }
    n=$(cat "$TEA_OUT.calls" 2>/dev/null || echo 0)
    echo $((n + 1)) >"$TEA_OUT.calls"
    if [ "$n" -ge 1 ] && [ -f "$TEA_PR_JSON2" ]; then cat "$TEA_PR_JSON2"; else cat "$TEA_PR_JSON"; fi ;;
  comment)
    [ "${TEA_COMMENT_RC:-0}" = 0 ] || exit "$TEA_COMMENT_RC"
    printf '%s\n' "$5" >"$TEA_OUT"; echo "$4" >"$TEA_OUT.number" ;;
  *) echo "tea shim: $*" >&2; exit 9 ;;
esac
SHIM
chmod +x "$cp_bin/tea"
pr_json() { printf '{"state":"%s","merged":%s,"head":{"sha":"%s"}}\n' "$1" "$2" "$3"; }
# run_pr <args>: tools/check-pr.sh in the scratch checkout; stdout and stderr in
# $root/pr.out, the exit code in pr_rc.
run_pr() {
  rm -f "$root/pr-comment.txt" "$root/pr-comment.txt.calls" "$root/pr-comment.txt.number"
  (cd "$cp_main" && PATH="$cp_bin:$PATH" HEAVY_LOCK="$root/pr.heavy" TEA_PR_JSON="$root/pr.json" \
    TEA_OUT="$root/pr-comment.txt" tools/check-pr.sh "$@") >"$root/pr.out" 2>&1
  pr_rc=$?
  return "$pr_rc"
}
wt=$cp_main/.claude/worktrees/check-pr-7
# ends_with_exit <code>: the last line of the output, the one a caller waits for.
ends_with_exit() { [[ $(tail -n 1 "$root/pr.out") == "check-pr: exit $1" ]]; }

run_pr --help
expect "--help prints the whole header, to its last line, and exits 0" \
  eval '[[ $pr_rc -eq 0 ]] && grep -qF "the end to wait for." "$root/pr.out" && ! grep -q "^set -uo" "$root/pr.out"'
for bad in "" abc 0 -3 "7 8" 07; do
  # shellcheck disable=SC2086
  run_pr $bad
  rc=$?
  expect "argument '$bad' is refused with exit 2 and a usage line" \
    eval '[[ $rc -eq 2 ]] && grep -q "^usage: tools/check-pr.sh" "$root/pr.out" && ends_with_exit 2'
done
pr_json closed true "$head1" >"$root/pr.json"
run_pr 7
expect "a merged pull request: exit 2, says so, no comment, no worktree" \
  eval '[[ $pr_rc -eq 2 ]] && grep -q "pull request 7 is merged" "$root/pr.out" && [[ ! -e $root/pr-comment.txt && ! -e $wt ]] && ends_with_exit 2'
pr_json closed false "$head1" >"$root/pr.json"
run_pr 7
expect "a closed pull request: exit 2, says so" eval '[[ $pr_rc -eq 2 ]] && grep -q "pull request 7 is closed" "$root/pr.out"'
pr_json open false "$head1" >"$root/pr.json"
TEA_API_FAIL=1 run_pr 7
expect "no server: exit 2, says so" eval '[[ $pr_rc -eq 2 ]] && grep -q "the server" "$root/pr.out"'
echo "not json" >"$root/pr.json"
run_pr 7
expect "an answer that is not a pull request: exit 2" eval '[[ $pr_rc -eq 2 ]] && grep -q "the server" "$root/pr.out"'
pr_json open false "$head1" >"$root/pr.json"
run_pr 99
expect "a pull request that was not pushed (nothing to fetch): exit 2, says git" \
  eval '[[ $pr_rc -eq 2 ]] && grep -q "git: cannot fetch pull/99/head" "$root/pr.out"'

FAKE_CHECK=pass run_pr 7
rc=$?
expect "a passed check: exit 0" test "$rc" -eq 0
expect "the output has the result line" grep -qx "Full check passed at $head1\." "$root/pr.out"
expect "the last line of the output is the exit code" ends_with_exit 0
expect "the comment starts with the exact line" test "$(head -n 1 "$root/pr-comment.txt")" = "Full check passed at $head1."
expect "the comment holds the table in a code block" \
  eval 'grep -q "^cargo-test    ok" "$root/pr-comment.txt" && [[ $(grep -c "^\`\`\`$" "$root/pr-comment.txt") -eq 2 ]]'
expect "the comment went to pull request 7" test "$(cat "$root/pr-comment.txt.number")" = 7
expect "the worktree is detached at the head" \
  eval '[[ $(git -C "$wt" rev-parse HEAD) == "$head1" && -z $(git -C "$wt" branch --show-current) ]]'
expect "the whole output is in tests/.data/check/run.log" grep -q "fake step" "$wt/tests/.data/check/run.log"

# A new head; the worktree is reused, ignored output stays, a stale client build goes.
mkdir -p "$wt/app/node_modules" "$wt/app/dist"
touch "$wt/app/node_modules/kept" "$wt/app/dist/stale"
echo second >>"$cp_main/app-types.txt"
git -C "$cp_main" -c user.name=t -c user.email=t@t commit -q -am second
head2=$(git -C "$cp_main" rev-parse HEAD)
git -C "$cp_origin" fetch -q "$cp_main" main
git -C "$cp_origin" update-ref refs/pull/7/head "$head2"
pr_json open false "$head2" >"$root/pr.json"
FAKE_CHECK=fail run_pr 7
rc=$?
expect "a failed check: exit 1" test "$rc" -eq 1
expect "and the last line says exit 1" ends_with_exit 1
expect "the worktree is reused at the new head" test "$(git -C "$wt" rev-parse HEAD)" = "$head2"
expect "ignored build output stays (node_modules)" test -e "$wt/app/node_modules/kept"
expect "the client build of the old head is removed" test ! -e "$wt/app/dist/stale"
expect "the comment names the failed step" test "$(head -n 1 "$root/pr-comment.txt")" = "Full check failed at $head2: cargo-test."
expect "the output has the first line and the absolute path of the log" \
  eval 'grep -qx "Full check failed at $head2: cargo-test\." "$root/pr.out" && grep -qF "$wt/tests/.data/check/cargo-test.log" "$root/pr.out"'
expect "the table with the skipped step is in the comment" grep -q "^build         skipped: --fast" "$root/pr-comment.txt"
expect "a file the failed check changed is restored" test "$(git -C "$wt" status --porcelain --untracked-files=no)" = ""
FAKE_CHECK=pass run_pr 7
expect "so the next run is not refused" test $? -eq 0

echo "stale" >"$wt/app-types.txt"
run_pr 7
expect "a worktree with tracked changes: exit 2, says so" eval '[[ $pr_rc -eq 2 ]] && grep -q "has tracked changes" "$root/pr.out"'
git -C "$wt" checkout -q -- .

pr_json open false "$sha_b" >"$root/pr2.json"
TEA_PR_JSON2="$root/pr2.json" FAKE_CHECK=pass run_pr 7
expect "a head that moved during the run: still the checked sha in the first line, one more line" \
  eval '[[ $(head -n 1 "$root/pr-comment.txt") == "Full check passed at $head2." ]] && grep -qF "moved during the run to $sha_b; this result is for $head2." "$root/pr-comment.txt"'

TEA_COMMENT_RC=1 FAKE_CHECK=pass run_pr 7
rc=$?
expect "a comment that fails: exit 2, the result is still printed" \
  eval '[[ $rc -eq 2 ]] && grep -qx "Full check passed at $head2\." "$root/pr.out" && grep -q "the comment on pull request 7 was not posted" "$root/pr.out" && ends_with_exit 2'

# A run of the same pull request is going: the lock of the pull request is taken.
(
  exec 9>"$root/check-pr-7.lock"
  flock -n 9 || exit 1
  FAKE_CHECK=pass run_pr 7
  rc=$?
  [[ $rc -eq 2 ]] && grep -q "another run of the check of pull request 7" "$root/pr.out" && [[ ! -e $root/pr-comment.txt ]]
)
expect "a second run for the same pull request: exit 2, says so, no comment" test $? -eq 0

git -C "$cp_main" worktree remove --force "$wt"
mkdir -p "$wt"
run_pr 7
expect "a folder that is not a worktree in the way: exit 2" eval '[[ $pr_rc -eq 2 ]] && grep -q "is not a worktree" "$root/pr.out"'
rmdir "$wt"
FAKE_CHECK=pass run_pr 7
expect "a removed worktree is created again" eval '[[ $pr_rc -eq 0 && $(git -C "$wt" rev-parse HEAD) == "$head2" ]]'

echo "tools/ci-runner.sh (no server: the argument handling, the generated texts, install and uninstall against tea and systemctl shims):"
tools/ci-runner.sh >"$root/ci.out" 2>&1
rc=$?
expect "without arguments it exits 2 with the usage" eval '[[ $rc -eq 2 ]] && grep -q "^usage: tools/ci-runner.sh install|uninstall|status" "$root/ci.out"'
tools/ci-runner.sh restart >"$root/ci.out" 2>&1
expect "an unknown command exits 2" test $? -eq 2
tools/ci-runner.sh status extra >"$root/ci.out" 2>&1
expect "an extra argument exits 2" test $? -eq 2
tools/ci-runner.sh --help >"$root/ci.out" 2>&1
rc=$?
expect "--help exits 0 and prints the header, to its last line" \
  eval '[[ $rc -eq 0 ]] && grep -q "^  *tools/ci-runner.sh install" "$root/ci.out" && tail -n 1 "$root/ci.out" | grep -q "^Needs: "'

ci_conf=$root/ci/config ci_data=$root/ci/data ci_units=$root/ci/units ci_bin=$root/ci/bin ci_sh=$root/ci/shims ci_main=$root/ci/main
mkdir -p "$ci_sh" "$ci_main/tools" "$ci_bin" "$root/ci/state"
printf '#!/bin/sh\n' >"$ci_main/tools/check-pr.sh"
chmod +x "$ci_main/tools/check-pr.sh"
# The runner binary of the tests: only its version (the pinned one, so nothing is downloaded).
pinned=$(sed -n 's/^RUNNER_VERSION=//p' tools/ci-runner.sh)
printf '#!/bin/sh\necho "forgejo-runner version v%s"\n' "$pinned" >"$ci_bin/forgejo-runner"
chmod +x "$ci_bin/forgejo-runner"
# tea: `api --login owner [-X METHOD] [-d BODY] ENDPOINT` on a list of runners in
# $CI_STATE/runners.json; every call is logged in $CI_STATE/tea.log.
cat >"$ci_sh/tea" <<'SHIM'
#!/bin/sh
[ "$1" = api ] || { echo "tea shim: $*" >&2; exit 9; }
shift 3
method=GET
while [ $# -gt 1 ]; do
  case "$1" in -X) method=$2 ;; -d) body=$2 ;; esac
  shift 2
done
echo "$method $1 $body" >>"$CI_STATE/tea.log"
[ -z "$CI_NO_SERVER" ] || { echo "tea: no server (test)" >&2; exit 1; }
n=$(cat "$CI_STATE/count" 2>/dev/null || echo 0)
case "$method $1" in
  "GET /repos/{owner}/{repo}/actions/runners")
    cat "$CI_STATE/runners.json" 2>/dev/null || echo '[{"id":1,"uuid":"u-other","name":"someone-else","status":"idle","version":"v1","labels":["x"]}]' ;;
  "POST /repos/{owner}/{repo}/actions/runners")
    n=$((n + 1)); echo "$n" >"$CI_STATE/count"
    printf '[{"id":%s,"uuid":"u-%s","name":"baluk-host","status":"idle","version":"v13","labels":["baluk-host"]}]\n' "$n" "$n" >"$CI_STATE/runners.json"
    printf '{"id":%s,"uuid":"u-%s","token":"tok-%s"}\n' "$n" "$n" "$n" ;;
  "DELETE /repos/{owner}/{repo}/actions/runners/"*)
    echo '[]' >"$CI_STATE/runners.json" ;;
  *) echo "tea shim: $method $1" >&2; exit 9 ;;
esac
SHIM
# systemctl --user: a service that is active or not; every call is logged.
cat >"$ci_sh/systemctl" <<'SHIM'
#!/bin/sh
shift
echo "$*" >>"$CI_STATE/systemctl.log"
case "$1" in
  show-environment | daemon-reload | enable | cat) exit 0 ;;
  is-active) [ -e "$CI_STATE/active" ] && { [ "$2" = --quiet ] || echo active; exit 0; }; [ "$2" = --quiet ] || echo inactive; exit 3 ;;
  is-enabled) echo enabled ;;
  start | restart) : >"$CI_STATE/active" ;;
  disable) rm -f "$CI_STATE/active" ;;
esac
SHIM
chmod +x "$ci_sh/tea" "$ci_sh/systemctl"
# run_ci <command>: tools/ci-runner.sh against the shims and the scratch places.
run_ci() {
  CI_STATE=$root/ci/state PATH="$ci_sh:$PATH" CI_RUNNER_CONFIG_DIR=$ci_conf CI_RUNNER_DATA_DIR=$ci_data \
    CI_RUNNER_UNIT_DIR=$ci_units CI_RUNNER_BIN_DIR=$ci_bin CI_RUNNER_ONLINE_WAIT=2 BALUK_MAIN=${CI_TEST_MAIN:-$ci_main} \
    tools/ci-runner.sh "$@" >"$root/ci.out" 2>&1
}
ci_fn() { (source tools/ci-runner.sh; CONFIG=$ci_conf/config.yml "$@"); }

ci_fn config_text /main http://forge uuid-1 secret-1 >"$root/ci-config.txt"
expect "the config: one label, host execution, capacity 1" \
  eval 'grep -qx "  labels: \[\"baluk-host:host\"\]" "$root/ci-config.txt" && grep -qx "  capacity: 1" "$root/ci-config.txt"'
expect "the config: the main checkout for the jobs, a job longer than 90 minutes" \
  eval 'grep -qx "    BALUK_MAIN: \"/main\"" "$root/ci-config.txt" && grep -qx "  timeout: 4h" "$root/ci-config.txt"'
expect "the config: the connection of the runner" \
  eval 'grep -qx "      url: \"http://forge\"" "$root/ci-config.txt" && grep -qx "      uuid: \"uuid-1\"" "$root/ci-config.txt" && grep -qx "      token: \"secret-1\"" "$root/ci-config.txt"'
mkdir -p "$ci_conf"
cp "$root/ci-config.txt" "$ci_conf/config.yml"
expect "the uuid and the token are read back from the config" \
  eval '[[ $(ci_fn conn_field uuid) == uuid-1 && $(ci_fn conn_field token) == secret-1 ]]'
rm -rf "$ci_conf"
expect "a missing config gives no uuid and no message" eval '[[ -z $(ci_fn conn_field uuid 2>&1) ]]'
ci_fn unit_text /bin/runner /conf.yml /p:/q >"$root/ci-unit.txt"
expect "the unit: start line, PATH, nice, restart, the default target" \
  eval 'grep -qx "ExecStart=/bin/runner daemon --config /conf.yml" "$root/ci-unit.txt" && grep -qx "Environment=PATH=/p:/q" "$root/ci-unit.txt" && grep -qx "Nice=2" "$root/ci-unit.txt" && grep -qx "Restart=on-failure" "$root/ci-unit.txt" && grep -qx "WantedBy=default.target" "$root/ci-unit.txt"'
printf '[{"id":3,"uuid":"u-3","name":"baluk-host","status":"idle","version":"v13","labels":["baluk-host"]},{"id":4,"uuid":"u-4","name":"other","status":"offline","version":null,"labels":[]}]' |
  ci_fn parse_runners >"$root/ci-runners.txt"
expect "only the runner of our name is read, with its id, uuid, status, version and labels" \
  test "$(cat "$root/ci-runners.txt")" = "3 u-3 idle v13 baluk-host"
ci_fn put_file "$root/ci-put.txt" 600 <<<"one"
expect "put_file: a new file is a change, with its mode" eval '[[ $(cat "$root/ci-put.txt") == one && $(stat -c %a "$root/ci-put.txt") == 600 ]]'
ci_fn put_file "$root/ci-put.txt" 600 <<<"one"
expect "put_file: the same text and mode is no change" test $? -eq 1
ci_fn put_file "$root/ci-put.txt" 644 <<<"one"
expect "put_file: another mode is a change" test $? -eq 0
ci_fn put_file "$root/ci-put.txt" 644 <<<"two"
rc=$?
expect "put_file: another text is a change" eval '[[ $rc -eq 0 &&$(cat "$root/ci-put.txt") == two ]]'

run_ci status
expect "status before install: exit 1" test $? -eq 1
run_ci install
rc=$?
expect "install: exit 0, registers the runner, writes the config and the unit, starts the service" \
  eval '[[ $rc -eq 0 ]] && grep -q "^POST /repos/{owner}/{repo}/actions/runners {\"name\":\"baluk-host\"" "$root/ci/state/tea.log" && [[ -f $ci_conf/config.yml && -f $ci_units/forgejo-runner.service ]] && grep -qx "start forgejo-runner.service" "$root/ci/state/systemctl.log" && grep -q "is online" "$root/ci.out"'
expect "install: the config has the token of the registration, is private, names the main checkout" \
  eval '[[ $(stat -c %a "$ci_conf/config.yml") == 600 ]] && grep -qx "      token: \"tok-1\"" "$ci_conf/config.yml" && grep -qx "    BALUK_MAIN: \"$ci_main\"" "$ci_conf/config.yml"'
expect "install: the token is not printed" eval '! grep -q tok-1 "$root/ci.out"'
expect "install: the runner of someone else is not touched" eval '! grep -q "^DELETE" "$root/ci/state/tea.log"'
: >"$root/ci/state/tea.log"
: >"$root/ci/state/systemctl.log"
cp "$ci_conf/config.yml" "$root/ci-config-1.yml"
run_ci install
rc=$?
expect "install again: exit 0, no registration, no restart, nothing rewritten" \
  eval '[[ $rc -eq 0 ]] && ! grep -q "^POST\|^DELETE" "$root/ci/state/tea.log" && ! grep -q "start\|restart\|daemon-reload" "$root/ci/state/systemctl.log" && cmp -s "$ci_conf/config.yml" "$root/ci-config-1.yml" && ! grep -q "wrote" "$root/ci.out"'
run_ci status
rc=$?
expect "status after install: exit 0, names the unit and the runner" \
  eval '[[ $rc -eq 0 ]] && grep -q "^unit: .*active" "$root/ci.out" && grep -q "^runner:  baluk-host id 1 idle" "$root/ci.out"'
# A changed text (another main checkout) rewrites the config and restarts the service.
: >"$root/ci/state/systemctl.log"
mkdir -p "$ci_main/other/tools" && printf '#!/bin/sh\n' >"$ci_main/other/tools/check-pr.sh" && chmod +x "$ci_main/other/tools/check-pr.sh"
CI_TEST_MAIN=$ci_main/other run_ci install
rc=$?
expect "install with another main checkout: the config is rewritten, the service restarted, the same runner kept" \
  eval '[[ $rc -eq 0 ]] && grep -qx "restart forgejo-runner.service" "$root/ci/state/systemctl.log" && grep -qx "    BALUK_MAIN: \"$ci_main/other\"" "$ci_conf/config.yml" && ! grep -q "^POST" "$root/ci/state/tea.log"'
# The runner is gone from Forgejo: a new registration replaces the config's one.
echo '[]' >"$root/ci/state/runners.json"
run_ci install
rc=$?
expect "install when Forgejo lost the runner: registered again, the new token in the config" \
  eval '[[ $rc -eq 0 ]] && grep -qx "      token: \"tok-2\"" "$ci_conf/config.yml"'
# A runner of our name that the config does not know (a lost config) is replaced.
rm -rf "$ci_conf"
run_ci install
rc=$?
expect "install without the config but with a runner of the name: it is removed, a new one registered" \
  eval '[[ $rc -eq 0 ]] && grep -q "^DELETE /repos/{owner}/{repo}/actions/runners/2" "$root/ci/state/tea.log" && grep -qx "      token: \"tok-3\"" "$ci_conf/config.yml"'
CI_NO_SERVER=1 run_ci install
rc=$?
expect "install without the server: exit 1, the server is named" eval '[[ $rc -eq 1 ]] && grep -q "cannot list the runners" "$root/ci.out"'
rm -f "$ci_main/tools/check-pr.sh"
run_ci install
rc=$?
expect "install outside the main checkout: exit 1, says so" eval '[[ $rc -eq 1 ]] && grep -q "check-pr.sh is missing" "$root/ci.out"'
printf '#!/bin/sh\n' >"$ci_main/tools/check-pr.sh"
chmod +x "$ci_main/tools/check-pr.sh"
run_ci uninstall
rc=$?
expect "uninstall: exit 0, the service stopped, the unit, the runner in Forgejo, the config, the data and the binary gone" \
  eval '[[ $rc -eq 0 ]] && grep -q "^disable --now" "$root/ci/state/systemctl.log" && [[ ! -e $ci_units/forgejo-runner.service && ! -e $ci_conf && ! -e $ci_data && ! -e $ci_bin/forgejo-runner ]] && [[ $(cat "$root/ci/state/runners.json") == "[]" ]]'
run_ci uninstall
expect "uninstall again: exit 0" test $? -eq 0

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
PATH=$wb/path run_wb "$wb/bundle"
rc=$?
expect "web-bundle accepts an earlier bundle, and names a missing tool before it builds" \
  eval '[[ $rc -eq 1 ]] && grep -q "node not found" "$root/wb.out" && [[ -f $wb/bundle/index.html ]]'

echo
if (( fails == 0 )); then
  echo "libs tests passed"
else
  echo "$fails libs test(s) FAILED"
fi
exit "$fails"
