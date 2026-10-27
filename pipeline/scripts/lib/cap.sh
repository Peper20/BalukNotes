# Sourced by the kit's scripts and by the project's own (pipeline/docs/README.md,
# "Checks"): the CPU cap and the lock that keeps parallel runs within it.
#
# CHECK_JOBS is the number of logical CPUs the scripts may keep busy; never
# more than the machine has. Unset, it is all of a machine with 1-2 CPUs and
# otherwise about five sixths of it: n - round(n/6) (12 -> 10, 6 -> 5, 4 -> 3).
# It is exported, and the project's hook file (`check.env` of the config) turns
# it into the setting of each tool it runs.

source "$(dirname "${BASH_SOURCE[0]}")/config.sh"

# default_check_jobs <cpus>: the cap of a machine with <cpus> logical CPUs.
# round(n/6), halves up, is (n + 3) / 6 in integers.
default_check_jobs() {
  local n=$1
  if (( n <= 2 )); then
    echo "$n"
  else
    echo $(( n - (n + 3) / 6 ))
  fi
}

cpus=$(nproc 2>/dev/null) || cpus=${CHECK_JOBS:-1}
[[ $cpus =~ ^[1-9][0-9]*$ ]] || cpus=1
CHECK_JOBS=${CHECK_JOBS:-$(default_check_jobs "$cpus")}
if [[ ! $CHECK_JOBS =~ ^[1-9][0-9]*$ ]]; then
  echo "CHECK_JOBS must be a positive integer, got: $CHECK_JOBS" >&2
  exit 2
fi
(( CHECK_JOBS > cpus )) && CHECK_JOBS=$cpus
unset cpus
export CHECK_JOBS

# below <n>: CHECK_JOBS - n, at least 1.
below() {
  local n=$((CHECK_JOBS - $1))
  echo $(( n < 1 ? 1 : n ))
}

# One lock for all heavy work of all runs on the machine: steps of different
# runs take turns, so together they stay within the cap. flock is released by
# the kernel when its holder dies, so a killed run does not block the others;
# the child processes do not inherit the lock, so one that outlives its step (a
# server left running) does not hold it either. `check.heavyLock` of the config,
# else ~/.cache/<project name as a word>/heavy.lock.
if [[ -z ${HEAVY_LOCK:-} ]]; then
  HEAVY_LOCK=$(config_get check.heavyLock)
  if [[ -n $HEAVY_LOCK ]]; then
    HEAVY_LOCK=$(project_path "$HEAVY_LOCK")
  else
    HEAVY_LOCK=$(project_cache_dir)/heavy.lock
  fi
fi

# The project's hook file: the settings of its tools from CHECK_JOBS, and
# `on_heavy_acquire`, called when the lock is taken.
CHECK_ENV=$(config_get check.env)
if [[ -n $CHECK_ENV ]]; then
  source "$(project_path "$CHECK_ENV")"
fi

# heavy_acquire: wait for the lock (say so, if it is taken; HEAVY_WAIT is the
# seconds spent waiting); heavy_release. The record in $HEAVY_LOCK.last is the
# checkout that held the lock last; `on_heavy_acquire <changed>` of the project
# is called under the lock, with `changed` = 1 for the first holder after a run
# of another checkout (that run may have built its own tree into a shared
# place) and 0 for a repeated run of the same checkout.
heavy_acquire() {
  local here changed=0
  HEAVY_WAIT=0
  mkdir -p "$(dirname "$HEAVY_LOCK")"
  exec {HEAVY_FD}>"$HEAVY_LOCK"
  if ! flock -n "$HEAVY_FD"; then
    echo "  waiting for another run (lock $HEAVY_LOCK)" >&2
    local since=$SECONDS
    flock "$HEAVY_FD"
    HEAVY_WAIT=$((SECONDS - since))
  fi
  here=$(git rev-parse --show-toplevel 2>/dev/null) || here=$PWD
  if [[ $(cat "$HEAVY_LOCK.last" 2>/dev/null) != "$here" ]]; then
    echo "$here" >"$HEAVY_LOCK.last"
    changed=1
  fi
  if declare -F on_heavy_acquire >/dev/null; then
    on_heavy_acquire "$changed"
  fi
}
heavy_release() {
  exec {HEAVY_FD}>&-
}

# heavy <command...>: the command under the lock; its exit code is returned.
heavy() {
  local rc
  heavy_acquire
  "$@" {HEAVY_FD}>&-
  rc=$?
  heavy_release
  return "$rc"
}
