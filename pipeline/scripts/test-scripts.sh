#!/usr/bin/env bash
# Tests of the kit's scripts: the runner check.sh (the API of a steps file, the
# table, the summary with the wait for the lock, GITHUB_STEP_SUMMARY), the cap
# and the lock (lib/cap.sh), test.sh (the mapping of paths to commands,
# --dry-run), measure.sh, the reading of the config and the check of the mod
# with its engine types (lib/mod-check.sh). They run in scratch
# projects (a git repository with a .claude/pipeline.json, a steps file and an
# env hook of their own) made in a temporary folder; the project that holds
# the kit is not touched, and its lock is not used.
#
#   pipeline/scripts/test-scripts.sh
set -uo pipefail
cd "$(dirname "$0")/.."
kit=$PWD/scripts
# Outside the project: a run inside a CI job must not write into its summary.
unset GITHUB_STEP_SUMMARY PIPELINE_PROJECT_ROOT HEAVY_LOCK CHECK_JOBS

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT
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

# new_project <dir> [name]: a scratch project with one commit on main, origin/main
# at that commit, and the config of the tests below.
new_project() {
  mkdir -p "$1/.claude" "$1/ci"
  cat >"$1/.claude/pipeline.json" <<JSON
{
  "project": "${2:-Scratch Project}",
  "forge": { "url": "http://forge.test", "repo": "acme/scratch" },
  "check": { "steps": "ci/steps.sh", "env": "ci/env.sh", "logs": "out/check", "heavyLock": "$1/heavy.lock" },
  "tests": [
    { "match": "^lib/([^/]+)/", "run": "echo unit-\$1" },
    { "match": "^lib/([^/]+)/x/", "run": "echo extra-\$1" },
    { "match": "^doc/", "run": "echo docs" },
    { "match": "^bad/", "run": "echo bad; exit 3" }
  ]
}
JSON
  : >"$1/ci/env.sh"
  git init -q -b main "$1"
  git -C "$1" add -A
  git -C "$1" -c user.name=t -c user.email=t@t commit -q -m first
  git -C "$1" update-ref refs/remotes/origin/main HEAD
}

# --- the cap and the lock ---
echo "the CPU cap (lib/cap.sh):"
p=$root/cap
new_project "$p"
mkdir -p "$root/nproc-bin"
printf '#!/bin/sh\necho "$NPROC_SHIM"\n' >"$root/nproc-bin/nproc"
chmod +x "$root/nproc-bin/nproc"
# cap <cpus> [CHECK_JOBS]: CHECK_JOBS and `below 1` on a machine with that many CPUs.
cap() {
  (
    cd "$p" && export NPROC_SHIM=$1 PATH="$root/nproc-bin:$PATH"
    unset CHECK_JOBS
    [[ -z ${2:-} ]] || export CHECK_JOBS=$2
    source "$kit/lib/cap.sh"
    echo "$CHECK_JOBS $(below 1) $(below 5)"
  )
}
expect "1 CPU: the cap is 1" test "$(cap 1)" = "1 1 1"
expect "2 CPUs: all of them" test "$(cap 2)" = "2 1 1"
expect "3 CPUs: 2" test "$(cap 3)" = "2 1 1"
expect "4 CPUs: 3" test "$(cap 4)" = "3 2 1"
expect "6 CPUs: 5" test "$(cap 6)" = "5 4 1"
expect "8 CPUs: 7" test "$(cap 8)" = "7 6 2"
expect "9 CPUs: 7 (1.5 rounds up)" test "$(cap 9)" = "7 6 2"
expect "12 CPUs: 10" test "$(cap 12)" = "10 9 5"
expect "16 CPUs: 13" test "$(cap 16)" = "13 12 8"
expect "an explicit CHECK_JOBS wins" test "$(cap 12 4)" = "4 3 1"
expect "an explicit CHECK_JOBS is capped at the CPUs of the machine" test "$(cap 6 20)" = "6 5 1"
(cd "$p" && export NPROC_SHIM=4 PATH="$root/nproc-bin:$PATH" CHECK_JOBS=lots; source "$kit/lib/cap.sh") >"$root/cap-bad.log" 2>&1
rc=$?
expect "CHECK_JOBS that is not a number is refused with exit 2" \
  eval '[[ $rc -eq 2 ]] && grep -q "CHECK_JOBS must be a positive integer" "$root/cap-bad.log"'

echo "the project's hook file and the lock:"
cat >"$p/ci/env.sh" <<'HOOK'
export TOOL_THREADS=$(below 1)
on_heavy_acquire() { echo "acquired changed=$1" >>"$ACQUIRE_LOG"; }
HOOK
(
  cd "$p" && export ACQUIRE_LOG=$root/acquire.log CHECK_JOBS=3
  source "$kit/lib/cap.sh"
  echo "$TOOL_THREADS" >"$root/threads"
  heavy true
  heavy true
  echo /some/other/checkout >"$HEAVY_LOCK.last"
  heavy true
)
expect "the hook file sees CHECK_JOBS and below" test "$(cat "$root/threads")" = 2
expect "on_heavy_acquire gets 1 for the first holder and after another checkout, 0 for a repeated run" \
  test "$(paste -sd' ' "$root/acquire.log")" = "acquired changed=1 acquired changed=0 acquired changed=1"
expect "the record is the checkout's path" test "$(cat "$p/heavy.lock.last")" = "$(cd "$p" && git rev-parse --show-toplevel)"
expect "the lock of the config is used" test -f "$p/heavy.lock"
: >"$p/ci/env.sh"
expect "heavy returns the exit code of the command" \
  eval '(cd "$p" && source "$kit/lib/cap.sh" && heavy sh -c "exit 7"); [[ $? -eq 7 ]]'
expect "the lock is held while the command runs" \
  eval '(cd "$p" && source "$kit/lib/cap.sh" && heavy sh -c '\''exec 8>"$1"; ! flock -n 8'\'' _ "$HEAVY_LOCK")'
expect "the lock is free after it" \
  eval '(cd "$p" && source "$kit/lib/cap.sh" && heavy true; exec 8>"$HEAVY_LOCK"; flock -n 8)'
expect "a process the command leaves running does not hold the lock" \
  eval '(cd "$p" && source "$kit/lib/cap.sh" && heavy sh -c '\''sleep 2 >/dev/null 2>&1 &'\''; exec 8>"$HEAVY_LOCK"; flock -n 8)'
# lock_path <project dir> [NAME=value...]: HEAVY_LOCK of a project.
lock_path() { (cd "$1" && shift && env -u XDG_CACHE_HOME -u HEAVY_LOCK "$@" bash -c 'source "$0"; echo "$HEAVY_LOCK"' "$kit/lib/cap.sh"); }
p2=$root/named
new_project "$p2" "My Project!"
sed -i 's/, "heavyLock": "[^"]*"//' "$p2/.claude/pipeline.json"
expect "without heavyLock: ~/.cache/<project name as a word>/heavy.lock" \
  test "$(lock_path "$p2" HOME=/h)" = "/h/.cache/my-project/heavy.lock"
expect "XDG_CACHE_HOME moves that default" test "$(lock_path "$p2" HOME=/h XDG_CACHE_HOME=/x)" = "/x/my-project/heavy.lock"
expect "HEAVY_LOCK of the environment wins" test "$(lock_path "$p2" HEAVY_LOCK=/own/lock)" = "/own/lock"
sed -i 's#"check": {#"check": { "heavyLock": "~/locks/l",#' "$p2/.claude/pipeline.json"
expect "a heavyLock with ~/ is under the home" test "$(lock_path "$p2" HOME=/h)" = "/h/locks/l"
sed -i 's#"heavyLock": "~/locks/l",#"heavyLock": "rel/l",#' "$p2/.claude/pipeline.json"
expect "a relative heavyLock is under the project root" test "$(lock_path "$p2")" = "$p2/rel/l"
p3=$root/noconfig
mkdir -p "$p3"
git init -q -b main "$p3"
expect "without a config: ~/.cache/pipeline/heavy.lock" test "$(lock_path "$p3" HOME=/h)" = "/h/.cache/pipeline/heavy.lock"

echo "the config without jq:"
mkdir -p "$root/nojq"
printf '#!/bin/sh\nexit 9\n' >"$root/nojq/jq"
chmod +x "$root/nojq/jq"
# cfg <function> <args...>: a function of lib/config.sh in the first scratch project, with a jq that fails.
cfg() { (cd "$p" && PATH="$root/nojq:$PATH" bash -c 'source "$0"; "$@"' "$kit/lib/config.sh" "$@"); }
expect "a jq that fails: python3 reads the key" test "$(cfg config_get check.logs)" = out/check
expect "and the rules of tests" test "$(cfg config_tests | wc -l)" -eq 4
expect "a missing key is empty" test -z "$(cfg config_get check.nothing)"
expect "jq itself gives the same key" test "$(cd "$p" && bash -c 'source "$0"; config_get check.logs' "$kit/lib/config.sh")" = out/check

# --- the runner ---
echo "check.sh (the runner):"
r=$root/runner
new_project "$r"
cat >"$r/ci/steps.sh" <<'STEPS'
before_steps() { echo "before" >>"$MARKS"; }
first() { echo "first ran"; }
failing() { for i in $(seq 1 30); do echo "line $i"; done; printf '\033[31mred\033[0m last\n'; return 1; }
niceness() { ps -o ni= -p $$ | tr -d ' ' >"$MARKS.nice"; }
steps() {
  case "${1:-}" in
    -h) echo "project help"; exit 0 ;;
    --bad) echo "bad option" >&2; exit 2 ;;
  esac
  printf '%s\n' "$@" >"$MARKS.args"
  step first first
  step failing failing
  if last_ok; then echo ran-after-failure >>"$MARKS"; fi
  skip build "--fast"
  step_unlocked unlocked niceness
  step "a|b" true
}
STEPS
export MARKS=$root/marks
run_check() { (cd "$r" && HEAVY_LOCK= "$kit/check.sh" "$@"); }
mkdir -p "$r/out/check"
echo stale >"$r/out/check/stale.log"
run_check one "two words" >"$root/check.out" 2>&1
rc=$?
expect "a failed step makes the exit code 1" test "$rc" -eq 1
expect "the arguments of the runner reach steps" test "$(cat "$MARKS.args")" = "$(printf 'one\ntwo words')"
expect "before_steps ran once, before the steps" test "$(cat "$MARKS")" = "before"
expect "last_ok is false after a failed step" eval '! grep -q ran-after-failure "$MARKS"'
expect "the old logs are gone, each step has its log" \
  eval '[[ ! -e $r/out/check/stale.log && $(cat "$r/out/check/first.log") == "first ran" ]]'
expect "the tail of a failed step is printed" \
  eval 'grep -q "failing failed - tail of out/check/failing.log" "$root/check.out" && grep -q "line 30" "$root/check.out"'
expect "the table on stdout: the header, an ok, a failed and a skipped step" \
  eval 'grep -qx "step          result                time, s" "$root/check.out" &&
    grep -Eq "^first +ok +[0-9]+$" "$root/check.out" && grep -Eq "^failing +failed +[0-9]+$" "$root/check.out" &&
    grep -Eq "^build +skipped: --fast +-$" "$root/check.out"'
table_out=$(sed -n '/^step /,/^$/p' "$root/check.out" | sed '/^$/d')
expect "table.txt is that table" test "$table_out" = "$(cat "$r/out/check/table.txt")"
expect "the last line says it failed and where the logs are" \
  test "$(tail -n 1 "$root/check.out" | sed 's/([0-9]* s)/(N s)/')" = "CHECK FAILED (N s): logs in out/check/"
s=$r/out/check/summary.md
expect "summary.md: a title with the project and the verdict" eval 'head -n 1 "$s" | grep -Eq "^## Scratch Project: FAILED in [0-9]+ s$"'
expect "summary.md: the table has the lock wait column" grep -qx '| step | result | time, s | lock wait, s |' "$s"
expect "summary.md: a row per step, a skipped step has no numbers, a | in a name is kept" \
  eval 'grep -Eq "^\| first \| ok \| [0-9]+ \| [0-9]+ \|$" "$s" && grep -qx "| build | skipped: --fast | - | - |" "$s" && grep -Eq "^\| a\|b \| ok \|" "$s"'
expect "summary.md: a total row and the wall time" \
  eval 'grep -Eq "^\| \*\*total\*\* \| \| [0-9]+ \| [0-9]+ \|$" "$s" && grep -Eq "^Wall time of the run: [0-9]+ s\.$" "$s"'
expect "summary.md: a collapsed block with the last 25 lines of the failed step, colors removed" \
  eval 'grep -q "<details><summary>failing: the last 25 lines of its log</summary>" "$s" && grep -qx "line 30" "$s" &&
    ! grep -qx "line 5" "$s" && grep -qx "red last" "$s" && ! grep -q "first ran" "$s"'
expect "a passed step has no block" eval '! grep -q "<summary>first" "$s"'
expect "the step ran with the priority of the runner (nice 2 or lower)" eval '[[ $(cat "$MARKS.nice") -ge 2 ]]'

rm -f "$MARKS"
run_check -h >"$root/help.out" 2>&1
rc=$?
expect "-h is the project's: its help, exit 0, no hook, logs untouched" \
  eval '[[ $rc -eq 0 && $(cat "$root/help.out") == "project help" && ! -e $MARKS && -f $r/out/check/summary.md ]]'
run_check --bad >"$root/bad.out" 2>&1
rc=$?
expect "a wrong option of the project: exit 2, no hook, logs untouched" \
  eval '[[ $rc -eq 2 && ! -e $MARKS && -f $r/out/check/summary.md ]]'

echo "check.sh, a passing run, the lock held by another run, GITHUB_STEP_SUMMARY:"
cat >"$r/ci/steps.sh" <<'STEPS'
steps() {
  step waited true
  step quick true
}
STEPS
printf 'earlier job text\n' >"$root/job-summary.md"
# Another run holds the lock for 3 s.
(exec 9>"$r/heavy.lock"; flock 9; sleep 3) &
holder=$!
until ! flock -n 8 8>"$r/heavy.lock"; do sleep 0.1; done
(cd "$r" && GITHUB_STEP_SUMMARY=$root/job-summary.md "$kit/check.sh") >"$root/pass.out" 2>&1
rc=$?
wait "$holder"
expect "a passing run exits 0" test "$rc" -eq 0
expect "the wait for the lock is announced" grep -q "waiting for another run" "$root/pass.out"
s=$r/out/check/summary.md
expect "the verdict is passed, no block of a failure" eval 'head -n 1 "$s" | grep -Eq "passed in [0-9]+ s$" && ! grep -q "<details>" "$s"'
expect "the wait of the first step is in the lock wait column, not in its time" \
  eval '[[ $(awk -F"|" "\$2 == \" waited \" { gsub(/ /, \"\", \$4); gsub(/ /, \"\", \$5); print \$4 \" \" \$5 }" "$s") =~ ^0\ [1-3]$ ]]'
expect "a step that did not wait has no lock wait" \
  eval '[[ $(awk -F"|" "\$2 == \" quick \" { gsub(/ /, \"\", \$5); print \$5 }" "$s") == 0 ]]'
expect "the total row sums the lock wait" eval 'grep -Eq "^\| \*\*total\*\* \| \| 0 \| [1-3] \|$" "$s"'
expect "the run says it appended the summary, after the table" \
  eval 'grep -qx "summary: appended to $root/job-summary.md" "$root/pass.out" && [[ $(tail -n 1 "$root/pass.out") == "check passed ("* ]]'
expect "GITHUB_STEP_SUMMARY gets the summary after what it had" \
  eval '[[ $(head -n 1 "$root/job-summary.md") == "earlier job text" ]] && grep -q "^| step | result | time, s | lock wait, s |$" "$root/job-summary.md"'

echo "check.sh, a project without steps:"
cp "$r/.claude/pipeline.json" "$root/pipeline.json.keep"
sed -i 's#"steps": "ci/steps.sh", ##' "$r/.claude/pipeline.json"
(cd "$r" && "$kit/check.sh") >"$root/nosteps.out" 2>&1
rc=$?
expect "no check.steps: exit 2 and the reason" eval '[[ $rc -eq 2 ]] && grep -q "\"check.steps\" is not set" "$root/nosteps.out"'
cp "$root/pipeline.json.keep" "$r/.claude/pipeline.json"
printf 'x=1\n' >"$r/ci/steps.sh"
(cd "$r" && "$kit/check.sh") >"$root/nofn.out" 2>&1
rc=$?
expect "a steps file without the function steps: exit 2" eval '[[ $rc -eq 2 ]] && grep -q "does not define the function steps" "$root/nofn.out"'

# --- test.sh ---
echo "test.sh (targeted tests):"
t=$root/targeted
new_project "$t"
dry() { (cd "$t" && "$kit/test.sh" --dry-run "$@" 2>&1); }
expect "a path gives the command of its rule, with the group of the match" test "$(dry lib/core/a.rs)" = "echo unit-core"
expect "two files of one group: the command once" test "$(dry lib/core/a.rs lib/core/b.rs)" = "echo unit-core"
expect "files of two groups: a command each, in the order of the paths" \
  test "$(dry lib/core/a.rs doc/x.md lib/app/b.rs)" = "$(printf 'echo unit-core\necho docs\necho unit-app')"
expect "a path that matches two rules gets both commands" test "$(dry lib/core/x/a.rs)" = "$(printf 'echo unit-core\necho extra-core')"
expect "a path no rule matches: nothing to run, exit 0" \
  eval '[[ $(dry README.md) == "no targeted tests for these changes (1 path(s))" ]]'
expect "a group with a space or a quote is quoted" test "$(dry "lib/a b'c/f")" = "echo unit-a\\ b\\'c"
# The paths from git: a commit after origin/main, a change not committed, a new file.
git -C "$t" switch -q -c work
mkdir -p "$t/lib/core" "$t/lib/ui" "$t/doc"
echo 1 >"$t/lib/core/a.rs"
git -C "$t" add -A
git -C "$t" -c user.name=t -c user.email=t@t commit -q -m committed
echo 2 >>"$t/lib/core/a.rs"
echo 3 >"$t/lib/ui/new.rs"
expect "without paths: the files changed against origin/main, committed, uncommitted and new" \
  test "$(dry)" = "$(printf 'echo unit-core\necho unit-ui')"
git -C "$t" add -A
git -C "$t" -c user.name=t -c user.email=t@t commit -q -m more
git -C "$t" update-ref refs/remotes/origin/main HEAD
expect "nothing changed against origin/main: nothing to run" eval '[[ $(dry) == "no targeted tests for these changes (0 path(s))" ]]'
(cd "$t" && "$kit/test.sh" lib/core/a.rs lib/core/b.rs doc/x.md) >"$root/targeted.out" 2>&1
rc=$?
expect "a run: each command once, all passed, exit 0, a table" \
  eval '[[ $rc -eq 0 && $(grep -c "^unit-core$" "$root/targeted.out") -eq 1 && $(grep -c "^docs$" "$root/targeted.out") -eq 1 ]] &&
    grep -Eq "^ok +[0-9]+ +echo unit-core$" "$root/targeted.out"'
(cd "$t" && "$kit/test.sh" bad/a doc/x.md) >"$root/targeted.out" 2>&1
rc=$?
expect "a failed command: the others still run, exit 1, the table says failed (3)" \
  eval '[[ $rc -eq 1 ]] && grep -q "^docs$" "$root/targeted.out" && grep -Eq "^failed \(3\) +[0-9]+ +echo bad; exit 3$" "$root/targeted.out"'
(cd "$t" && "$kit/test.sh" --nope) >"$root/targeted.out" 2>&1
expect "an unknown option: exit 2" test "$?" -eq 2
sed -i 's/"tests": \[/"tests-off": [/' "$t/.claude/pipeline.json"
(cd "$t" && "$kit/test.sh" --dry-run lib/core/a.rs) >"$root/targeted.out" 2>&1
rc=$?
expect "no rules in the config: exit 2 and the reason" eval '[[ $rc -eq 2 ]] && grep -q "no \"tests\"" "$root/targeted.out"'

# --- measure.sh ---
echo "measure.sh:"
m=$root/measure
new_project "$m"
measure() { (cd "$m" && MEASURE_INTERVAL=0.1 "$kit/measure.sh" "$@" 2>&1); }
out=$(measure 3 -- sh -c 'sleep 0.5')
rc=$?
expect "N runs are reported, each with wall seconds, busy CPUs and the exit code" \
  eval '[[ $rc -eq 0 && $(grep -Ec "^run [123]: [0-9]+\.[0-9]{2} s, avg [0-9]+\.[0-9]{2} peak [0-9]+\.[0-9]{2} busy CPUs, exit 0$" <<<"$out") -eq 3 ]]'
expect "then min, median and max of the wall seconds" \
  eval 'grep -Eq "^wall: min [0-9]+\.[0-9]{2} s, median [0-9]+\.[0-9]{2} s, max [0-9]+\.[0-9]{2} s$" <<<"$out"'
expect "the wall seconds are those of the command (0.5 s)" eval 'grep -Eq "^run 1: 0\.[5-9][0-9] s" <<<"$out"'
out=$(measure 2 -- sh -c 'exit 4')
rc=$?
expect "a failing command: its code in every run, exit 1, no sample for a run shorter than the interval" \
  eval '[[ $rc -eq 1 && $(grep -c "exit 4$" <<<"$out") -eq 2 ]] && grep -q "no sample" <<<"$out"'
out=$(measure 4 -- true)
expect "the median of an even number of runs is between the middle ones" eval 'grep -q "^wall: min" <<<"$out"'
cat >"$root/lock-held.sh" <<'SCRIPT'
#!/bin/sh
exec 8>"$1"
! flock -n 8
SCRIPT
cat >"$root/nice-of.sh" <<'SCRIPT'
#!/bin/sh
ps -o ni= -p $$ | tr -d ' ' >"$1"
SCRIPT
chmod +x "$root/lock-held.sh" "$root/nice-of.sh"
expect "the command runs under the lock of heavy work" \
  eval '(cd "$m" && "$kit/measure.sh" 1 -- "$root/lock-held.sh" "$m/heavy.lock" >/dev/null 2>&1)'
expect "the command runs at nice 2 or a lower priority" \
  eval '(cd "$m" && "$kit/measure.sh" 1 -- "$root/nice-of.sh" "$root/measure.nice" >/dev/null 2>&1); [[ $(cat "$root/measure.nice") -ge 2 ]]'
expect "a missing command part, runs that are not a number, a missing -- : exit 2" \
  eval '(cd "$m"; "$kit/measure.sh" 2 >/dev/null 2>&1; a=$?; "$kit/measure.sh" x -- true >/dev/null 2>&1; b=$?; "$kit/measure.sh" 0 -- true >/dev/null 2>&1; c=$?; "$kit/measure.sh" 2 true >/dev/null 2>&1; d=$?; [[ $a$b$c$d == 2222 ]])'

# --- lib/mod-check.sh: the check of the kit's mod and its engine types in a linked worktree ---
echo "lib/mod-check.sh (scratch checkouts: a main checkout with the types, a worktree of it, a checkout without them):"
pt=$root/types pt_types=pipeline/.claude-plugin/types
mkdir -p "$pt/bin"
# pt_repo <dir> <yes|no>: a checkout with one commit that holds the kit's libraries (tracked,
# as a project holds the kit); with yes, the types are laid as Claude Code lays them:
# untracked, and ignored by the .gitignore of their folder.
pt_repo() {
  mkdir -p "$1/pipeline/scripts"
  cp -r "$kit/lib" "$1/pipeline/scripts/"
  if [[ $2 == yes ]]; then
    mkdir -p "$1/$pt_types/claude-code"
    printf '{"compilerOptions":{}}\n' >"$1/$pt_types/tsconfig.json"
    printf '*\n' >"$1/$pt_types/.gitignore"
    printf 'export {};\n' >"$1/$pt_types/claude-code/index.d.ts"
  fi
  git init -q -b main "$1"
  git -C "$1" add -A
  git -C "$1" -c user.name=t -c user.email=t@t commit -q -m first
}
pt_repo "$pt/main" yes
pt_repo "$pt/none" no
git -C "$pt/main" worktree add -q --detach "$pt/wt"
git -C "$pt/none" worktree add -q --detach "$pt/wt-none"
# pt_run <checkout> <function> [args]: a function of lib/mod-check.sh in that checkout; its output in $pt/lay.out.
pt_run() {
  local dir=$1
  shift
  (cd "$dir" && source pipeline/scripts/lib/mod-check.sh && "$@") >"$pt/lay.out" 2>&1
}
pt_run "$pt/wt" lay_pipeline_types
rc=$?
expect "a worktree without the types gets a copy of the main checkout's and says so" \
  eval '[[ $rc -eq 0 && $(cat "$pt/lay.out") == *"copied from"* ]]'
expect "the copy is a copy, not a link, with the .gitignore of its folder" \
  eval '[[ -f $pt/wt/$pt_types/.gitignore && ! -L $pt/wt/$pt_types && ! -L $pt/wt/$pt_types/tsconfig.json ]] && cmp -s "$pt/main/$pt_types/tsconfig.json" "$pt/wt/$pt_types/tsconfig.json"'
expect "the copy is marked, and ignored by git: the worktree has no untracked file" \
  eval '[[ -f $pt/wt/$pt_types/.copied-from-main && -z $(git -C "$pt/wt" status --porcelain --untracked-files=all) ]] && git -C "$pt/wt" check-ignore -q "$pt_types/tsconfig.json" && git -C "$pt/wt" check-ignore -q "$pt_types/.copied-from-main"'
# a run replaces a copy that changed: an edited tsconfig.json and a stray file of ours go
echo changed >>"$pt/wt/$pt_types/tsconfig.json"
echo stray >"$pt/wt/$pt_types/stray.txt"
pt_run "$pt/wt" lay_pipeline_types
rc=$?
expect "a second run replaces a changed copy with the main checkout's, and says so" \
  eval '[[ $rc -eq 0 && ! -e $pt/wt/$pt_types/stray.txt && -f $pt/wt/$pt_types/.copied-from-main && $(cat "$pt/lay.out") == *"copied from"* ]] && cmp -s "$pt/main/$pt_types/tsconfig.json" "$pt/wt/$pt_types/tsconfig.json"'
# types laid by Claude Code in a linked worktree (no mark) are left alone
git -C "$pt/main" worktree add -q --detach "$pt/wt-laid"
mkdir -p "$pt/wt-laid/$pt_types"
echo laid >"$pt/wt-laid/$pt_types/tsconfig.json"
pt_run "$pt/wt-laid" lay_pipeline_types
rc=$?
expect "types laid by Claude Code in a linked worktree (no mark) are left alone" \
  eval '[[ $rc -eq 0 && $(cat "$pt/wt-laid/$pt_types/tsconfig.json") == laid && ! -e $pt/wt-laid/$pt_types/.copied-from-main && ! -s $pt/lay.out ]]'
pt_run "$pt/main" lay_pipeline_types
rc=$?
expect "the main checkout, which has the types, copies nothing onto itself" \
  eval '[[ $rc -eq 0 && ! -s $pt/lay.out && ! -e $pt/main/$pt_types/.copied-from-main ]]'
pt_run "$pt/wt-none" lay_pipeline_types
rc=$?
expect "neither checkout has the types: nothing is laid, no error" \
  eval '[[ $rc -eq 0 && ! -e $pt/wt-none/$pt_types && ! -s $pt/lay.out ]]'
mkdir -p "$pt/wt-none/$pt_types"
echo old >"$pt/wt-none/$pt_types/tsconfig.json"
: >"$pt/wt-none/$pt_types/.copied-from-main"
pt_run "$pt/wt-none" lay_pipeline_types
rc=$?
expect "a copy of ours goes when the main checkout has no types any more" \
  eval '[[ $rc -eq 0 && ! -e $pt/wt-none/$pt_types ]]'
# pipeline_tsc: the tsc of this checkout, else the main checkout's.
mkdir -p "$pt/main/deps/bin" "$pt/wt-none/deps/bin"
printf '#!/bin/sh\necho "tsc $*" >>"$PT_LOG"\n' >"$pt/main/deps/bin/tsc"
cp "$pt/main/deps/bin/tsc" "$pt/wt-none/deps/bin/tsc"
chmod +x "$pt/main/deps/bin/tsc" "$pt/wt-none/deps/bin/tsc"
pt_tsc() { (cd "$1" && source pipeline/scripts/lib/mod-check.sh && pipeline_tsc deps/bin/tsc); }
expect "a worktree without its own tsc takes the main checkout's" \
  eval '[[ $(pt_tsc "$pt/wt") == */types/main/deps/bin/tsc ]]'
expect "a checkout with its own tsc takes it first" test "$(pt_tsc "$pt/wt-none")" = "deps/bin/tsc"
# pipeline_mod_check with a claude shim that logs its arguments ($CLAUDE_SHIM_FAIL: the subcommand that fails).
printf '#!/bin/sh\necho "claude $*" >>"$PT_LOG"\n[ "$2" = "$CLAUDE_SHIM_FAIL" ] && exit 1\nexit 0\n' >"$pt/bin/claude"
chmod +x "$pt/bin/claude"
export PT_LOG=$pt/mod.log
mod_check() { (cd "$1" && PATH="$pt/bin:$PATH" bash -c 'source pipeline/scripts/lib/mod-check.sh; pipeline_mod_check deps/bin/tsc') >"$pt/mod.out" 2>&1; }
: >"$PT_LOG"
mod_check "$pt/main"
rc=$?
expect "the mod check: validate, tests and tsc on the kit's folder, in this order" \
  eval '[[ $rc -eq 0 && $(paste -sd, "$PT_LOG") == "claude plugin validate pipeline,claude plugin test pipeline,tsc -p pipeline" ]]'
: >"$PT_LOG"
CLAUDE_SHIM_FAIL=validate mod_check "$pt/main"
rc=$?
expect "a failed validation stops the check before the tests and tsc" \
  eval '[[ $rc -eq 1 && $(cat "$PT_LOG") == "claude plugin validate pipeline" ]]'
: >"$PT_LOG"
mod_check "$pt/wt-none"
rc=$?
expect "without the engine types tsc is left out with a line, not a failure" \
  eval '[[ $rc -eq 0 && $(paste -sd, "$PT_LOG") == "claude plugin validate pipeline,claude plugin test pipeline" ]] && grep -q "types of the engine are not laid" "$pt/mod.out"'

echo
if (( fails == 0 )); then
  echo "script tests passed"
else
  echo "$fails script test(s) FAILED"
fi
exit "$fails"
