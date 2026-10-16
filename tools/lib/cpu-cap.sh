# Sourced by tools/check.sh, install.sh and test-env.sh: the CPU cap and the
# lock that keeps parallel runs within it (`.claude/rules/tools.md`, "CPU cap").
#
# CHECK_JOBS (default 9) is the number of logical CPUs the scripts may keep
# busy; never more than the machine has. It is exported to every tool the
# scripts start, in the form each tool understands. Tools that use their own
# thread pools can overshoot a bare job count: the values below are the ones
# measured to keep the total at or under CHECK_JOBS + 1.

CHECK_JOBS=${CHECK_JOBS:-9}
if [[ ! $CHECK_JOBS =~ ^[1-9][0-9]*$ ]]; then
  echo "CHECK_JOBS must be a positive integer, got: $CHECK_JOBS" >&2
  exit 2
fi
cpus=$(nproc 2>/dev/null || echo "$CHECK_JOBS")
(( CHECK_JOBS > cpus )) && CHECK_JOBS=$cpus
unset cpus
export CHECK_JOBS

# below <n>: CHECK_JOBS - n, at least 1.
below() {
  local n=$((CHECK_JOBS - $1))
  echo $(( n < 1 ? 1 : n ))
}

# cargo: rustc processes, build scripts, C compilers (jobserver) and the
# codegen threads of every rustc, which take tokens from the same jobserver.
# One below the cap: at the end of a build the linker (lld, its threads are
# limited in .cargo/config.toml) and C compilers run next to the last rustc.
# Overrides `[build] jobs` of ~/.cargo/config.toml.
export CARGO_BUILD_JOBS=$(below 1)
# The libtest harness: tests of one binary in parallel (binaries run in turn).
# One below: the test threads, the rayon pool and the server threads of the
# tests add up to more than the thread count.
export RUST_TEST_THREADS=$(below 1)
# The global rayon pool (Typst layout in tests and in the e2e server, the
# bundler of Vite). Two below: the core also builds two themes at once, each
# in its own thread, next to the pool (`notes-core::world::PARALLEL`).
export RAYON_NUM_THREADS=$(below 2)
# The default tokio runtimes (servers started by tests and e2e).
export TOKIO_WORKER_THREADS=$CHECK_JOBS
# Vitest: the workers that run test files (the main process is one more).
export VITEST_MAX_WORKERS=$(below 1)

# One lock for all heavy work of all runs on the machine (the check of every
# worktree, the installation, the test server build): steps of different runs
# take turns, so together they stay within the cap. flock is released by the
# kernel when its holder dies, so a killed run does not block the others; the
# child processes do not inherit the lock, so one that outlives its step (a
# server left running) does not hold it either.
HEAVY_LOCK=${HEAVY_LOCK:-${XDG_CACHE_HOME:-$HOME/.cache}/baluk-notes/heavy.lock}

# freshen_sources: set the file times of every tracked file the Rust build can
# read to now. Cargo judges our own crates fresh by source mtime against its
# outputs, and the build dir is shared by all checkouts without the path in
# the unit hash: a build of another checkout, newer than these files, would be
# taken for ours (measured: a crate built from another worktree's edit was
# reused). With fresh times cargo recompiles our crates against this tree;
# dependencies from the registry and the foreign copies in vendor/ are not
# touched and stay built. Called under
# the lock by heavy_acquire.
freshen_sources() {
  git ls-files -z -- crates baluk fonts docs/writing.md Cargo.toml Cargo.lock .cargo clippy.toml |
    xargs -0 -r touch -c
}

# heavy_acquire: wait for the lock (say so, if it is taken); heavy_release.
# The first holder after a different run freshens the sources (above): a run
# that held the lock in between may have built its own tree into the shared dir.
heavy_acquire() {
  mkdir -p "$(dirname "$HEAVY_LOCK")"
  exec {HEAVY_FD}>"$HEAVY_LOCK"
  if ! flock -n "$HEAVY_FD"; then
    echo "  waiting for another run (lock $HEAVY_LOCK)" >&2
    flock "$HEAVY_FD"
  fi
  if [[ $(cat "$HEAVY_LOCK.last" 2>/dev/null) != "$$" ]]; then
    echo "$$" >"$HEAVY_LOCK.last"
    freshen_sources
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
