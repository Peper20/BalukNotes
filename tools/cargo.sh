#!/usr/bin/env bash
# cargo by hand with the settings of the checks (`.claude/rules/tools.md`,
# "Checks" and "CPU cap"): the shared build dir of a linked worktree, the CPU
# cap, and the lock of heavy work, which freshens the sources when another
# checkout held the lock last. A plain `cargo` in a worktree builds from scratch
# or reuses another checkout's artifacts of our crates.
#
#   tools/cargo.sh test -p notes-core
#   tools/cargo.sh --version
#
# Runs in the checkout root, as the checks do, so the arguments mean the same
# from any folder of the checkout. Exits with cargo's code, or 2 outside a
# checkout of this repository. Do not call it from inside a locked step
# (tools/check.sh, install.sh, test-env.sh): the lock is not reentrant and the
# call would wait forever.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2
root=$(git rev-parse --show-toplevel 2>/dev/null)
if [[ -z $root || ! -f $root/tools/lib/worktree-target.sh || ! -f $root/tools/lib/cpu-cap.sh || ! -f $root/tools/lib/build-env.sh ]]; then
  echo "tools/cargo.sh: not in a checkout of baluk notes" >&2
  exit 2
fi
cd "$root" || exit 2

# The same order as tools/check.sh.
source tools/lib/worktree-target.sh
source tools/lib/cpu-cap.sh
source tools/lib/build-env.sh

heavy nice -n 2 cargo "$@"
exit $?
