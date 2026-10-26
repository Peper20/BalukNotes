#!/usr/bin/env bash
# The runner of a project's full check (pipeline/docs/README.md, "Checks"). The
# steps are the project's: the file named by `check.steps` in
# .claude/pipeline.json defines the function `steps`, which gets the arguments
# of this script (so the project parses its own options and prints its own
# help) and runs the steps through the functions below.
#
#   step <name> <command...>     the command under the lock of heavy work; the
#                                result and the time (the wait is counted apart)
#   step_unlocked <name> <cmd>   the same for a command that takes the lock
#                                itself (the lock is not reentrant)
#   skip <name> <reason>
#   last_ok                      whether the step run last passed
#   $logs                        the folder of the logs (`check.logs`), one
#                                <step>.log per step
#
# Optional in the steps file: `before_steps`, run under no lock once, before the
# first step (it may use `heavy`); `on_heavy_acquire` of the cap (lib/cap.sh).
#
# All steps run even if one fails (the steps file decides which depend on
# another). At the end: a table of the steps on stdout and in $logs/table.txt,
# a Markdown summary $logs/summary.md (appended to $GITHUB_STEP_SUMMARY when it
# is set, as Forgejo Actions does) with the wait for the lock per step and the
# last lines of the log of each failed step. The exit code is non-zero if any
# step failed.
set -uo pipefail

# Lower than normal priority (the steps inherit it): a build on all cores
# yields to work at the computer (nice 2; ionice - scale 0-7, normal 4) and
# takes idle CPU just the same.
renice -n 2 -p $$ >/dev/null 2>&1 || true
ionice -c 2 -n 5 -p $$ >/dev/null 2>&1 || true

kit=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
source "$kit/lib/config.sh"
cd "$PROJECT_ROOT" || exit 2
source "$kit/lib/cap.sh"

steps_file=$(config_get check.steps)
if [[ -z $steps_file ]]; then
  echo "check.sh: \"check.steps\" is not set in .claude/pipeline.json (or neither jq nor python3 can read it)" >&2
  exit 2
fi
steps_file=$(project_path "$steps_file")
if [[ ! -f $steps_file ]]; then
  echo "check.sh: the steps file $steps_file does not exist" >&2
  exit 2
fi
source "$steps_file"
if ! declare -F steps >/dev/null; then
  echo "check.sh: $steps_file does not define the function steps" >&2
  exit 2
fi

logs=$(config_get check.logs)
logs=${logs:-.pipeline/check}
project=$(config_get project)

names=() results=() times=() waits=()
failed=0 began=0

# begin: before the first step (so that a wrong option or -h of the project
# changes nothing): its hook, then empty logs.
begin() {
  (( began )) && return
  began=1
  if declare -F before_steps >/dev/null; then
    before_steps
  fi
  rm -rf "$logs"
  mkdir -p "$logs"
}

run_step() {
  local locked="$1" name="$2"; shift 2
  local log="$logs/$name.log" start rc lockwait=0
  begin
  echo "▶ $name: $*"
  if [[ $locked == 1 ]]; then
    heavy_acquire
    lockwait=$HEAVY_WAIT
    start=$SECONDS
    "$@" >"$log" 2>&1 {HEAVY_FD}>&-
    rc=$?
    heavy_release
  else
    start=$SECONDS
    "$@" >"$log" 2>&1
    rc=$?
  fi
  if [[ $rc == 0 ]]; then
    results+=("ok")
  else
    results+=("failed")
    failed=1
    echo "  ✗ $name failed - tail of $log:"
    tail -n 25 "$log" | sed 's/^/    /'
  fi
  names+=("$name")
  times+=("$((SECONDS - start))")
  waits+=("$lockwait")
}
step() { run_step 1 "$@"; }
step_unlocked() { run_step 0 "$@"; }

# skip <name> <reason>
skip() {
  begin
  names+=("$1") results+=("skipped: $2") times+=("-") waits+=("-")
}

last_ok() {
  (( ${#results[@]} > 0 )) && [[ ${results[-1]} == ok ]]
}

steps "$@"

# printf pads by bytes, not by letters - pad ourselves.
cell() {
  local LC_ALL=C.UTF-8 s="$1" width="$2"
  printf '%s%*s' "$s" $((width - ${#s})) ''
}
table() {
  cell "step" 14; cell "result" 22; echo "time, s"
  local i
  for i in "${!names[@]}"; do
    cell "${names[$i]}" 14; cell "${results[$i]}" 22; echo "${times[$i]}"
  done
}

# summary: the Markdown of the run. Seconds that are not numbers (a skipped
# step) count as nothing.
summary() {
  local i total=0 waited=0 verdict="passed"
  (( failed )) && verdict="FAILED"
  echo "## ${project:-check}: $verdict in $SECONDS s"
  echo
  echo "| step | result | time, s | lock wait, s |"
  echo "|---|---|--:|--:|"
  for i in "${!names[@]}"; do
    echo "| ${names[$i]} | ${results[$i]//|/\\|} | ${times[$i]} | ${waits[$i]} |"
    [[ ${times[$i]} =~ ^[0-9]+$ ]] && total=$((total + times[i]))
    [[ ${waits[$i]} =~ ^[0-9]+$ ]] && waited=$((waited + waits[i]))
  done
  echo "| **total** | | $total | $waited |"
  echo
  echo "Wall time of the run: $SECONDS s."
  for i in "${!names[@]}"; do
    [[ ${results[$i]} == failed ]] || continue
    echo
    echo "<details><summary>${names[$i]}: the last 25 lines of its log</summary>"
    echo
    echo '````'
    tail -n 25 "$logs/${names[$i]}.log" | sed 's/\x1b\[[0-9;]*[A-Za-z]//g'
    echo '````'
    echo
    echo "</details>"
  done
}

echo
if (( began )); then
  table | tee "$logs/table.txt"
  summary >"$logs/summary.md"
  if [[ -n ${GITHUB_STEP_SUMMARY:-} ]]; then
    if cat "$logs/summary.md" >>"$GITHUB_STEP_SUMMARY"; then
      appended="summary: appended to $GITHUB_STEP_SUMMARY"
    else
      appended="check.sh: cannot write $GITHUB_STEP_SUMMARY"
    fi
  fi
else
  table
fi
echo
[[ -z ${appended:-} ]] || echo "$appended"
if [[ $failed == 0 ]]; then
  echo "check passed (${SECONDS} s)"
else
  echo "CHECK FAILED (${SECONDS} s): logs in $logs/"
fi
exit "$failed"
