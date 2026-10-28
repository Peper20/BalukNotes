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
unset GITHUB_STEP_SUMMARY PIPELINE_PROJECT_ROOT HEAVY_LOCK HEAVY_LABEL CHECK_JOBS

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

echo "the holder of the lock and the waiter's message:"
hp=$root/holder
new_project "$hp"
(
  cd "$hp" || exit 2
  HEAVY_LABEL="first run"
  source "$kit/lib/cap.sh"
  heavy_acquire
  sleep 2
  heavy_release
) &
first_pid=$!
until [[ -s $hp/heavy.lock.holder ]]; do sleep 0.1; done
expect "the holder file names the holder: its pid, the time it took the lock, its checkout and its label" \
  eval '[[ $(cat "$hp/heavy.lock.holder") == "$first_pid "[0-9][0-9]:[0-9][0-9]:[0-9][0-9]" $(cd "$hp" && git rev-parse --show-toplevel) first run" ]]'
(
  cd "$hp" || exit 2
  HEAVY_LABEL="second run"
  source "$kit/lib/cap.sh"
  heavy_acquire
  heavy_release
) 2>"$root/holder-wait.log"
wait "$first_pid"
expect "a waiter names the holder it waits for" \
  grep -Eq "^  waiting for another run \(lock $hp/heavy\.lock\): held by $first_pid [0-9:]{8} .* first run\$" "$root/holder-wait.log"
expect "a waiter that gets the lock says after how many seconds" grep -Eq '^  got the lock after [0-9]+ s$' "$root/holder-wait.log"
expect "the holder file is emptied when the holder releases" eval '[[ -z $(cat "$hp/heavy.lock.holder") ]]'
uk=$root/unknown-holder
new_project "$uk"
(exec 9>"$uk/heavy.lock"; flock 9; touch "$uk/ready"; sleep 2) &
unknown_pid=$!
until [[ -e $uk/ready ]]; do sleep 0.1; done
(cd "$uk" && source "$kit/lib/cap.sh" && heavy true) 2>"$root/unknown-wait.log"
wait "$unknown_pid"
expect "a lock held without a holder file is named as held by an unknown process" \
  grep -qx "  waiting for another run (lock $uk/heavy.lock): held by an unknown process" "$root/unknown-wait.log"
dl=$root/default-label
new_project "$dl"
printf '%s\n' 'source "$1"' 'heavy_acquire' 'cat "$HEAVY_LOCK.holder"' 'heavy_release' >"$root/label-caller.sh"
label_line=$(cd "$dl" && bash "$root/label-caller.sh" "$kit/lib/cap.sh")
expect "without HEAVY_LABEL the holder is named by the calling script" eval '[[ $label_line == *" label-caller.sh" ]]'
hn=$root/not-holder
new_project "$hn"
(cd "$hn" && source "$kit/lib/cap.sh" && heavy true)
expect "a command run under heavy empties the holder file" eval '[[ -z $(cat "$hn/heavy.lock.holder" 2>/dev/null) ]]'
(
  cd "$hn" || exit 2
  source "$kit/lib/cap.sh"
  heavy_acquire
  echo "4242 10:00:00 /elsewhere other" >"$HEAVY_LOCK.holder"
  heavy_release
)
expect "a release by a process that the file does not name leaves the file alone" \
  eval '[[ $(cat "$hn/heavy.lock.holder") == "4242 10:00:00 /elsewhere other" ]]'

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
expect "summary.md: a row per step, a skipped step has no numbers, a | in a name is escaped" \
  eval 'grep -Eq "^\| first \| ok \| [0-9]+ \| [0-9]+ \|$" "$s" && grep -qx "| build | skipped: --fast | - | - |" "$s" && grep -Eq "^\| a\\\\\|b \| ok \|" "$s"'
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
  step quick sh -c 'cat heavy.lock.holder >"$1"' _ "$HOLDER_SEEN"
}
STEPS
printf 'earlier job text\n' >"$root/job-summary.md"
# Another run holds the lock for 3 s.
(exec 9>"$r/heavy.lock"; flock 9; sleep 3) &
holder=$!
until ! flock -n 8 8>"$r/heavy.lock"; do sleep 0.1; done
(cd "$r" && HOLDER_SEEN=$root/holder-seen GITHUB_STEP_SUMMARY=$root/job-summary.md "$kit/check.sh") >"$root/pass.out" 2>&1
rc=$?
wait "$holder"
expect "a passing run exits 0" test "$rc" -eq 0
expect "the wait for the lock is announced" grep -q "waiting for another run" "$root/pass.out"
expect "a locked step is the holder while it runs, named by its step" grep -Eq ' check quick$' "$root/holder-seen"
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
# A locale with a decimal comma: EPOCHREALTIME reads "1700000000,123456" there, and LC_ALL beats LC_NUMERIC.
comma_locale=
while read -r l; do
  if [[ $(LC_ALL=$l locale decimal_point 2>/dev/null) == , ]]; then comma_locale=$l; break; fi
done < <(locale -a)
if [[ -n $comma_locale ]]; then
  out=$(LC_ALL=$comma_locale measure 1 -- sleep 1.2)
  expect "under a comma-decimal locale ($comma_locale) the wall seconds are still right (1.2 s)" \
    eval 'grep -Eq "^run 1: 1\.[2-5][0-9] s" <<<"$out"'
else
  echo "  skip  the comma-decimal locale test: no such locale in 'locale -a'"
fi
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
cat >"$root/holder-of.sh" <<'SCRIPT'
#!/bin/sh
cat "$1" >"$2"
SCRIPT
chmod +x "$root/lock-held.sh" "$root/nice-of.sh" "$root/holder-of.sh"
expect "the command runs under the lock of heavy work" \
  eval '(cd "$m" && "$kit/measure.sh" 1 -- "$root/lock-held.sh" "$m/heavy.lock" >/dev/null 2>&1)'
expect "the command runs at nice 2 or a lower priority" \
  eval '(cd "$m" && "$kit/measure.sh" 1 -- "$root/nice-of.sh" "$root/measure.nice" >/dev/null 2>&1); [[ $(cat "$root/measure.nice") -ge 2 ]]'
expect "measure names itself as the holder of the lock" \
  eval '(cd "$m" && "$kit/measure.sh" 1 -- "$root/holder-of.sh" "$m/heavy.lock.holder" "$root/measure.holder" >/dev/null 2>&1); grep -Eq " measure$" "$root/measure.holder"'
expect "a missing command part, runs that are not a number, a missing -- : exit 2" \
  eval '(cd "$m"; "$kit/measure.sh" 2 >/dev/null 2>&1; a=$?; "$kit/measure.sh" x -- true >/dev/null 2>&1; b=$?; "$kit/measure.sh" 0 -- true >/dev/null 2>&1; c=$?; "$kit/measure.sh" 2 true >/dev/null 2>&1; d=$?; [[ $a$b$c$d == 2222 ]])'

# --- check-pr.sh and ci-runner.sh ---
echo "check-pr.sh, the pieces (the output of check.sh is a sample):"
cat >"$root/sample-failed.out" <<'SAMPLE'
▶ cargo-test: cargo test --workspace
  ✗ cargo-test failed - tail of out/check/cargo-test.log:
    step    this line is indented, it is not the table

step          result                time, s
tools-libs    ok                    1
cargo-test    failed                42
fmt           ok                    3
build         skipped: --fast       -
e2e           failed                9

CHECK FAILED (55 s): logs in out/check/
SAMPLE
sed -e 's/failed  /ok      /' -e '/CHECK FAILED/d' "$root/sample-failed.out" >"$root/sample-passed.out"
echo "check passed (55 s)" >>"$root/sample-passed.out"
sha_a=$(printf 'a%.0s' {1..40})
sha_b=$(printf 'b%.0s' {1..40})
# fn <function> <args...>: a function of check-pr.sh, sourced in a subshell.
fn() { (source "$kit/check-pr.sh"; "$@"); }
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

echo "check-pr.sh (in a scratch checkout with its own origin, against tea and check.sh shims):"
cp_main=$root/pr-main cp_origin=$root/pr-origin.git cp_bin=$root/pr-bin
mkdir -p "$cp_main/pipeline/scripts" "$cp_main/.claude" "$cp_bin"
cp "$kit/check-pr.sh" "$cp_main/pipeline/scripts/"
cp -r "$kit/lib" "$cp_main/pipeline/scripts/"
echo '{ "project": "Scratch Project", "check": { "logs": "out/check" }, "ci": { "staleOutput": "dist" } }' >"$cp_main/.claude/pipeline.json"
# The check.sh of the pull request: FAKE_CHECK=pass|fail; it prints a table of
# steps like the real one, writes the log of a failed step and changes a
# tracked file as a failed api-types step does.
cat >"$cp_main/pipeline/scripts/check.sh" <<'SHIM'
#!/usr/bin/env bash
rm -rf out/check && mkdir -p out/check
echo "▶ fake step"
if [[ ${FAKE_CHECK:-pass} == fail ]]; then
  echo "boom" >out/check/cargo-test.log
  echo "stale" >exported.txt
  printf 'step          result                time, s\ncargo-test    failed                4\nbuild         skipped: --fast       -\n\nCHECK FAILED (4 s): logs in out/check/\n'
  exit 1
fi
printf 'step          result                time, s\ncargo-test    ok                    4\n\ncheck passed (4 s)\n'
SHIM
chmod +x "$cp_main/pipeline/scripts/check.sh"
echo fresh >"$cp_main/exported.txt"
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
# run_pr <args>: check-pr.sh in the scratch checkout; stdout and stderr in
# $root/pr.out, the exit code in pr_rc.
run_pr() {
  rm -f "$root/pr-comment.txt" "$root/pr-comment.txt.calls" "$root/pr-comment.txt.number"
  (cd "$cp_main" && PATH="$cp_bin:$PATH" HEAVY_LOCK="$root/pr.heavy" TEA_PR_JSON="$root/pr.json" \
    TEA_OUT="$root/pr-comment.txt" pipeline/scripts/check-pr.sh "$@") >"$root/pr.out" 2>&1
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
    eval '[[ $rc -eq 2 ]] && grep -q "^usage: pipeline/scripts/check-pr.sh" "$root/pr.out" && ends_with_exit 2'
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
expect "the whole output is in run.log of the logs folder (check.logs)" grep -q "fake step" "$wt/out/check/run.log"

# A new head; the worktree is reused, ignored output stays, the stale output goes.
mkdir -p "$wt/node_modules" "$wt/dist"
touch "$wt/node_modules/kept" "$wt/dist/stale"
echo second >>"$cp_main/exported.txt"
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
expect "ignored build output stays (node_modules)" test -e "$wt/node_modules/kept"
expect "the output named by ci.staleOutput is removed for a new head" test ! -e "$wt/dist/stale"
expect "the comment names the failed step" test "$(head -n 1 "$root/pr-comment.txt")" = "Full check failed at $head2: cargo-test."
expect "the output has the first line and the absolute path of the log" \
  eval 'grep -qx "Full check failed at $head2: cargo-test\." "$root/pr.out" && grep -qF "$wt/out/check/cargo-test.log" "$root/pr.out"'
expect "the table with the skipped step is in the comment" grep -q "^build         skipped: --fast" "$root/pr-comment.txt"
expect "a file the failed check changed is restored" test "$(git -C "$wt" status --porcelain --untracked-files=no)" = ""
FAKE_CHECK=pass run_pr 7
expect "so the next run is not refused" test $? -eq 0

echo "stale" >"$wt/exported.txt"
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

echo "ci-runner.sh (no server: the argument handling, the generated texts, install and uninstall against tea and systemctl shims):"
"$kit/ci-runner.sh" >"$root/ci.out" 2>&1
rc=$?
expect "without arguments it exits 2 with the usage" eval '[[ $rc -eq 2 ]] && grep -q "^usage: pipeline/scripts/ci-runner.sh install|uninstall|status" "$root/ci.out"'
"$kit/ci-runner.sh" restart >"$root/ci.out" 2>&1
expect "an unknown command exits 2" test $? -eq 2
"$kit/ci-runner.sh" status extra >"$root/ci.out" 2>&1
expect "an extra argument exits 2" test $? -eq 2
"$kit/ci-runner.sh" --help >"$root/ci.out" 2>&1
rc=$?
expect "--help exits 0 and prints the header, to its last line" \
  eval '[[ $rc -eq 0 ]] && grep -q "^  *pipeline/scripts/ci-runner.sh install" "$root/ci.out" && tail -n 1 "$root/ci.out" | grep -q "^Needs: "'

ci_conf=$root/ci/config ci_data=$root/ci/data ci_units=$root/ci/units ci_bin=$root/ci/bin ci_sh=$root/ci/shims ci_main=$root/ci/main
ci_proj=$root/ci/project
mkdir -p "$ci_sh" "$ci_main/pipeline/scripts" "$ci_bin" "$root/ci/state" "$ci_proj/.claude"
printf '#!/bin/sh\n' >"$ci_main/pipeline/scripts/check-pr.sh"
chmod +x "$ci_main/pipeline/scripts/check-pr.sh"
# The project of the script: the server, the name of the runner and its tools.
echo '{ "project": "Scratch Project", "forge": { "url": "http://forge.test", "repo": "acme/scratch" }, "ci": { "runner": "acme-host", "tools": "no-such-tool-xyz" } }' >"$ci_proj/.claude/pipeline.json"
# The runner binary of the tests: only its version (the pinned one, so nothing is downloaded).
pinned=$(sed -n 's/^RUNNER_VERSION=//p' "$kit/ci-runner.sh")
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
    printf '[{"id":%s,"uuid":"u-%s","name":"acme-host","status":"idle","version":"v13","labels":["acme-host"]}]\n' "$n" "$n" >"$CI_STATE/runners.json"
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
# run_ci <command>: ci-runner.sh against the shims and the scratch places.
run_ci() {
  CI_STATE=$root/ci/state PATH="$ci_sh:$PATH" CI_RUNNER_CONFIG_DIR=$ci_conf CI_RUNNER_DATA_DIR=$ci_data \
    CI_RUNNER_UNIT_DIR=$ci_units CI_RUNNER_BIN_DIR=$ci_bin CI_RUNNER_ONLINE_WAIT=2 PIPELINE_MAIN=${CI_TEST_MAIN:-$ci_main} PIPELINE_PROJECT_ROOT=${CI_TEST_PROJECT:-$ci_proj} \
    "$kit/ci-runner.sh" "$@" >"$root/ci.out" 2>&1
}
ci_fn() { (PIPELINE_PROJECT_ROOT=$ci_proj; source "$kit/ci-runner.sh"; CONFIG=$ci_conf/config.yml "$@"); }

ci_fn config_text /main http://forge uuid-1 secret-1 >"$root/ci-config.txt"
expect "the config: one label, host execution, capacity 1" \
  eval 'grep -qx "  labels: \[\"acme-host:host\"\]" "$root/ci-config.txt" && grep -qx "  capacity: 1" "$root/ci-config.txt"'
expect "the config: the main checkout for the jobs, a job longer than 90 minutes" \
  eval 'grep -qx "    PIPELINE_MAIN: \"/main\"" "$root/ci-config.txt" && grep -qx "  timeout: 4h" "$root/ci-config.txt"'
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
printf '[{"id":3,"uuid":"u-3","name":"acme-host","status":"idle","version":"v13","labels":["acme-host"]},{"id":4,"uuid":"u-4","name":"other","status":"offline","version":null,"labels":[]}]' |
  ci_fn parse_runners >"$root/ci-runners.txt"
expect "only the runner of our name is read, with its id, uuid, status, version and labels" \
  test "$(cat "$root/ci-runners.txt")" = "3 u-3 idle v13 acme-host"
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
  eval '[[ $rc -eq 0 ]] && grep -q "^POST /repos/{owner}/{repo}/actions/runners {\"name\":\"acme-host\"" "$root/ci/state/tea.log" && [[ -f $ci_conf/config.yml && -f $ci_units/forgejo-runner.service ]] && grep -qx "start forgejo-runner.service" "$root/ci/state/systemctl.log" && grep -q "is online" "$root/ci.out"'
expect "install: the config has the token of the registration, is private, names the main checkout" \
  eval '[[ $(stat -c %a "$ci_conf/config.yml") == 600 ]] && grep -qx "      token: \"tok-1\"" "$ci_conf/config.yml" && grep -qx "    PIPELINE_MAIN: \"$ci_main\"" "$ci_conf/config.yml"'
expect "install: the token is not printed" eval '! grep -q tok-1 "$root/ci.out"'
expect "install: the label is the runner's name from the config, the unit and the registration name the project" \
  eval 'grep -qx "  labels: \[\"acme-host:host\"\]" "$ci_conf/config.yml" && grep -qx "Description=Forgejo Runner of Scratch Project (full check of pull requests, host execution)" "$ci_units/forgejo-runner.service" && grep -q "\"description\":\"Scratch Project CI" "$root/ci/state/tea.log"'
expect "install: a tool of ci.tools that is missing is a warning, not a failure" \
  eval 'grep -q "WARNING: not found for the jobs: no-such-tool-xyz" "$root/ci.out"'
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
  eval '[[ $rc -eq 0 ]] && grep -q "^unit: .*active" "$root/ci.out" && grep -q "^runner:  acme-host id 1 idle" "$root/ci.out"'
# A changed text (another main checkout) rewrites the config and restarts the service.
: >"$root/ci/state/systemctl.log"
mkdir -p "$ci_main/other/pipeline/scripts" && printf '#!/bin/sh\n' >"$ci_main/other/pipeline/scripts/check-pr.sh" && chmod +x "$ci_main/other/pipeline/scripts/check-pr.sh"
CI_TEST_MAIN=$ci_main/other run_ci install
rc=$?
expect "install with another main checkout: the config is rewritten, the service restarted, the same runner kept" \
  eval '[[ $rc -eq 0 ]] && grep -qx "restart forgejo-runner.service" "$root/ci/state/systemctl.log" && grep -qx "    PIPELINE_MAIN: \"$ci_main/other\"" "$ci_conf/config.yml" && ! grep -q "^POST" "$root/ci/state/tea.log"'
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
rm -f "$ci_main/pipeline/scripts/check-pr.sh"
run_ci install
rc=$?
expect "install outside the main checkout: exit 1, says so" eval '[[ $rc -eq 1 ]] && grep -q "check-pr.sh is missing" "$root/ci.out"'
printf '#!/bin/sh\n' >"$ci_main/pipeline/scripts/check-pr.sh"
chmod +x "$ci_main/pipeline/scripts/check-pr.sh"
run_ci uninstall
rc=$?
expect "uninstall: exit 0, the service stopped, the unit, the runner in Forgejo, the config, the data and the binary gone" \
  eval '[[ $rc -eq 0 ]] && grep -q "^disable --now" "$root/ci/state/systemctl.log" && [[ ! -e $ci_units/forgejo-runner.service && ! -e $ci_conf && ! -e $ci_data && ! -e $ci_bin/forgejo-runner ]] && [[ $(cat "$root/ci/state/runners.json") == "[]" ]]'
run_ci uninstall
expect "uninstall again: exit 0" test $? -eq 0
# A project without ci.runner, or without the server: a clear error, nothing is called or written.
ci_noname=$root/ci/noname
mkdir -p "$ci_noname/.claude"
echo '{ "project": "Scratch Project", "forge": { "url": "http://forge.test", "repo": "acme/scratch" } }' >"$ci_noname/.claude/pipeline.json"
: >"$root/ci/state/tea.log"
: >"$root/ci/state/systemctl.log"
for c in install uninstall status; do
  CI_TEST_PROJECT=$ci_noname run_ci "$c"
  rc=$?
  expect "$c without ci.runner: exit 1, names the key, calls nothing, writes nothing" \
    eval '[[ $rc -eq 1 ]] && grep -q "\"ci.runner\" (the name of the runner) is not set in $ci_noname/.claude/pipeline.json" "$root/ci.out" && [[ ! -s $root/ci/state/tea.log && ! -s $root/ci/state/systemctl.log && ! -e $ci_conf && ! -e $ci_units/forgejo-runner.service ]]'
done
echo '{ "project": "Scratch Project", "ci": { "runner": "acme-host" } }' >"$ci_noname/.claude/pipeline.json"
CI_TEST_PROJECT=$ci_noname run_ci install
rc=$?
expect "install without forge.url: exit 1, names the key" eval '[[ $rc -eq 1 ]] && grep -q "\"forge.url\" is not set" "$root/ci.out" && [[ ! -s $root/ci/state/tea.log ]]'

# --- cloud-setup.sh and cloud-finish.sh ---
echo "cloud-setup.sh and cloud-finish.sh (a scratch project and its own copy of the scripts; the refusals only, no server is started):"
cl=$root/cloud
new_project "$cl" "My Project!"
mkdir -p "$cl/pipeline" "$root/cloud-home"
cp -r "$kit" "$cl/pipeline/scripts"
cl_port=$(python3 -I -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
# cloud <script> [args]: a script of the copy, with a home and a port of its own; output in $root/cloud.out.
cloud() {
  (cd "$cl" && env -u FORGEJO_HOME -u XDG_CACHE_HOME HOME="$root/cloud-home" FORGEJO_PORT="$cl_port" "pipeline/scripts/$@") >"$root/cloud.out" 2>&1
}
cloud cloud-setup.sh --stop
rc=$?
expect "the data dir is ~/.cache/<project name as a word>/forgejo" \
  eval '[[ $rc -eq 0 && $(cat "$root/cloud.out") == "no Forgejo of $root/cloud-home/.cache/my-project/forgejo is running" ]]'
cloud cloud-setup.sh
rc=$?
expect "setup in a clone without origin: exit 1, says so" eval '[[ $rc -eq 1 ]] && grep -q "has no remote .origin." "$root/cloud.out"'
git -C "$cl" remote add origin http://forge.test/acme/scratch.git
cloud cloud-setup.sh
rc=$?
expect "setup when origin is already the repository of the project's Forgejo: exit 1, nothing touched" \
  eval '[[ $rc -eq 1 ]] && grep -q "already the repository of a Forgejo" "$root/cloud.out" && [[ ! -e $root/cloud-home/.cache ]]'
git -C "$cl" remote set-url origin https://github.test/acme/scratch.git
git -C "$cl" remote add github https://github.test/other.git
cloud cloud-setup.sh
rc=$?
expect "a GitHub clone with the same owner and name is not taken for the Forgejo (the next refusal is the existing github remote)" \
  eval '[[ $rc -eq 1 ]] && grep -q "remote .github. exists" "$root/cloud.out"'
cloud cloud-finish.sh --branch main
rc=$?
expect "finish refuses main of GitHub" eval '[[ $rc -eq 1 ]] && grep -q "refusing to write to .main. of GitHub" "$root/cloud.out"'
cloud cloud-finish.sh --branch 'a b'
rc=$?
expect "finish refuses a name that is not a branch name" eval '[[ $rc -eq 1 ]] && grep -q "is not a valid branch name" "$root/cloud.out"'
cloud cloud-finish.sh --branch work
rc=$?
expect "finish when origin is not the cloud Forgejo: exit 1, says so" eval '[[ $rc -eq 1 ]] && grep -q "origin is not the cloud Forgejo (http://localhost:$cl_port/acme/scratch.git)" "$root/cloud.out"'
sed -i 's#"repo": "acme/scratch"#"repo": ""#' "$cl/.claude/pipeline.json"
cloud cloud-setup.sh
rc=$?
expect "setup without forge.repo: exit 1, names the key" eval '[[ $rc -eq 1 ]] && grep -q "\"forge.repo\" (owner/name) is not set" "$root/cloud.out"'
cloud cloud-finish.sh
rc=$?
expect "finish without forge.repo: exit 1, names the key" eval '[[ $rc -eq 1 ]] && grep -q "\"forge.repo\" (owner/name) is not set" "$root/cloud.out"'

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
