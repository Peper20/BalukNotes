#!/usr/bin/env bash
# The full check of a pull request, once per head commit (pipeline/docs/README.md,
# "Forgejo"): runs the full pipeline/scripts/check.sh in the worktree
# .claude/worktrees/check-pr-<number> of the main checkout, detached at the head
# of the pull request, and comments the result on the pull request as `dev`.
#
#   pipeline/scripts/check-pr.sh <number>
#
# The comment starts with exactly `Full check passed at <sha>.` or
# `Full check failed at <sha>: <failed steps, comma-separated>.`, then the step
# table of check.sh in a code block. The same first line is printed, and on a
# failure the absolute paths of the logs of the failed steps. The worktree stays
# (no branch; a later run reuses it - the ignored build output such as
# node_modules saves the work); the pipeline mod removes it with the task's
# cleanup. The whole output of check.sh is in run.log in the folder of the logs
# (`check.logs` of .claude/pipeline.json) of the worktree.
#
# Exit codes: 0 passed, 1 failed, 2 could not run (it says what: the argument,
# the server, git, the worktree, the comment). A second run for the same pull
# request while one is going exits 2. A comment that fails after a finished run
# still prints the result. The last line of the output is always
# `check-pr: exit <code>` (on stdout): the end to wait for.
set -uo pipefail

# parse_pr: the JSON of the pull request on stdin -> "<state> <merged> <head sha>".
parse_pr() {
  python3 -c '
import json, sys
d = json.load(sys.stdin)
print(d["state"], str(d["merged"]).lower(), d["head"]["sha"])
'
}

# step_table <output file>: the table of steps from the output of check.sh.
step_table() {
  awk '/^step +result +time/ {on = 1} on && /^$/ {exit} on {print}' "$1"
}

# failed_steps <output file>: the names of the failed steps, one per line.
failed_steps() {
  awk '/^step +result +time/ {on = 1; next} on && /^$/ {exit} on && $2 == "failed" {print $1}' "$1"
}

# first_line <sha> <failed steps, comma-separated; empty - passed>
first_line() {
  if [[ -z $2 ]]; then
    echo "Full check passed at $1."
  else
    echo "Full check failed at $1: $2."
  fi
}

# comment_text <sha> <failed steps, comma-separated> <table file> [<new head sha>]
comment_text() {
  local sha=$1 failed=$2 table=$3 moved=${4:-}
  first_line "$sha" "$failed"
  if [[ -n $moved ]]; then
    echo
    echo "The head of the pull request moved during the run to $moved; this result is for $sha."
  fi
  echo
  echo '```'
  if [[ -s $table ]]; then cat "$table"; else echo "(no table of steps in the output of check.sh)"; fi
  echo '```'
}

# wt_setting <worktree> <key>: a key of the worktree's own .claude/pipeline.json
# (the config of the pull request being checked).
wt_setting() {
  (PROJECT_ROOT=$1; config_get "$2")
}

# clear_stale_output <worktree>: the build output of an earlier head that the
# tests of this one would take for theirs (`ci.staleOutput`, paths against the
# worktree).
clear_stale_output() {
  local paths p
  read -ra paths <<<"$(wt_setting "$1" ci.staleOutput)"
  for p in "${paths[@]}"; do
    case $p in /* | *..*) continue ;; esac
    rm -rf "${1:?}/$p"
  done
}

die() {   # die <what could not run>: exit 2
  echo "check-pr: could not run: $*" >&2
  exit 2
}

main() {
  case "${1:-}" in
    -h|--help) awk 'NR > 1 && !/^#/ {exit} NR > 1 {sub(/^# ?/, ""); print}' "$0"; exit 0 ;;
  esac
  trap 'echo "check-pr: exit $?"' EXIT
  if [[ $# -ne 1 || ! $1 =~ ^[1-9][0-9]{0,8}$ ]]; then
    echo "usage: pipeline/scripts/check-pr.sh <pull request number>" >&2
    exit 2
  fi
  local n=$1 kit here main_root
  kit=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P) || die "the script's folder"
  here=$(cd "$kit/../.." && pwd) || die "the script's folder"
  # The main checkout (also when started from a linked worktree): the worktree
  # of the check lives next to the agents' ones.
  main_root=$(git -C "$here" rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || die "git: not in a checkout"
  main_root=$(dirname "$main_root")
  cd "$main_root" || die "the main checkout $main_root"
  source "$kit/lib/cap.sh"
  command -v python3 >/dev/null || die "python3 is missing (the answer of the server is read with it)"

  # One run per pull request. The lock is not passed to check.sh.
  local lock_fd
  mkdir -p "$(dirname "$HEAVY_LOCK")"
  exec {lock_fd}>"$(dirname "$HEAVY_LOCK")/check-pr-$n.lock"
  if ! flock -n "$lock_fd"; then
    die "another run of the check of pull request $n is going"
  fi

  local pr state merged sha
  pr=$(tea api --login dev "/repos/{owner}/{repo}/pulls/$n" 2>/dev/null) || die "the server: no answer for pull request $n (tea api --login dev)"
  read -r state merged sha < <(parse_pr <<<"$pr" 2>/dev/null) || die "the server: the answer for pull request $n is not a pull request"
  if [[ $merged == true ]]; then die "pull request $n is merged"; fi
  if [[ $state != open ]]; then die "pull request $n is $state"; fi

  git fetch -q origin "pull/$n/head" || die "git: cannot fetch pull/$n/head"
  # The commit that is fetched is the one checked, and named in the comment.
  sha=$(git rev-parse --verify -q 'FETCH_HEAD^{commit}') || die "git: no commit fetched for pull request $n"

  local wt=$main_root/.claude/worktrees/check-pr-$n old=
  git worktree prune
  if git worktree list --porcelain | grep -Fxq "worktree $wt"; then
    [[ -d $wt ]] || die "the worktree $wt is registered but missing"
    if [[ -n $(git -C "$wt" status --porcelain --untracked-files=no) ]]; then
      die "the worktree $wt has tracked changes (remove it: git worktree remove --force $wt)"
    fi
    old=$(git -C "$wt" rev-parse HEAD) || die "git: the worktree $wt"
    git -C "$wt" checkout -q --detach "$sha" || die "git: cannot check out $sha in $wt"
    # The client build of an earlier head would be served to the tests of this one.
    [[ $old == "$sha" ]] || clear_stale_output "$wt"
  else
    [[ ! -e $wt ]] || die "$wt exists and is not a worktree"
    git worktree add -q --detach "$wt" "$sha" || die "git: cannot create the worktree $wt"
  fi

  # check.sh clears the folder of the logs at the start: its output is kept
  # beside and moved there at the end.
  local logs out rc
  logs=$(wt_setting "$wt" check.logs)
  logs=$(PROJECT_ROOT=$wt project_path "${logs:-.pipeline/check}")
  out=$(dirname "$logs")/check-pr-run.log
  mkdir -p "$(dirname "$logs")"
  echo "check-pr: pull request $n at $sha, worktree $wt"
  (cd "$wt" && pipeline/scripts/check.sh {lock_fd}>&-) 2>&1 | tee "$out"
  rc=${PIPESTATUS[0]}
  mkdir -p "$logs"
  mv -f "$out" "$logs/run.log"
  out=$logs/run.log
  # A failed step may leave tracked files changed: the worktree is ours,
  # restore it so that the next run is not refused for tracked changes.
  git -C "$wt" checkout -q -- . 2>/dev/null

  local failed=() failed_csv=""
  mapfile -t failed < <(failed_steps "$out")
  if (( rc != 0 && ${#failed[@]} == 0 )); then failed=("check.sh exit $rc"); fi
  if (( ${#failed[@]} > 0 )); then
    failed_csv=$(IFS=,; echo "${failed[*]}" | sed 's/,/, /g')
  fi

  # Did the head move while the check ran?
  local moved="" pr2 state2 merged2 sha2
  if pr2=$(tea api --login dev "/repos/{owner}/{repo}/pulls/$n" 2>/dev/null) &&
    read -r state2 merged2 sha2 < <(parse_pr <<<"$pr2" 2>/dev/null) && [[ $sha2 != "$sha" ]]; then
    moved=$sha2
  fi

  local table=$logs/table.txt body
  step_table "$out" >"$table"
  body=$(comment_text "$sha" "$failed_csv" "$table" "$moved")

  echo
  first_line "$sha" "$failed_csv"
  [[ -z $moved ]] || echo "The head of the pull request moved during the run to $moved; this result is for $sha."
  local s
  for s in "${failed[@]}"; do
    [[ -f $logs/$s.log ]] && echo "  log: $logs/$s.log"
  done
  echo "  whole output: $out"

  if ! tea comment --login dev "$n" "$body" >/dev/null; then
    echo "check-pr: could not run: the comment on pull request $n was not posted (the result is above)" >&2
    exit 2
  fi
  (( ${#failed[@]} == 0 )) || exit 1
  exit 0
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  main "$@"
fi
