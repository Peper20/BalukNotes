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
if [[ -z $root || ! -f $root/tools/lib/worktree-target.sh || ! -f $root/tools/lib/cpu-cap.sh || ! -f $root/pipeline/scripts/lib/cap.sh || ! -f $root/tools/lib/build-env.sh ]]; then
  echo "tools/cargo.sh: not in a checkout of baluk notes" >&2
  exit 2
fi
cd "$root" || exit 2

# The same order as tools/check.sh.
source tools/lib/worktree-target.sh
source tools/lib/cpu-cap.sh
source tools/lib/build-env.sh

# copy_client: the server tests serve the client from app/dist (notes-assets reads
# it from disk), and a linked worktree has none. When its app/ is the main
# checkout's HEAD, the main checkout's build is copied in; otherwise the client
# must be built here, and that is said. A copy, not a link: a build in the
# worktree empties app/dist first, and through a link that would be the main one.
copy_client() {
  local common main head
  common=$(git rev-parse --path-format=absolute --git-common-dir)
  [[ $(git rev-parse --path-format=absolute --git-dir) == "$common" || -e app/dist ]] && return 0
  main=$(dirname "$common")
  head=$(git --git-dir="$common" rev-parse HEAD)
  if [[ -z $(git ls-files --others --exclude-standard -- app) ]] && git diff --quiet "$head" -- app; then
    if [[ -f $main/app/dist/index.html ]]; then
      cp -a "$main/app/dist" app/dist
      echo "tools/cargo.sh: app/dist is a copy of the main checkout's client build; a later change in app/ does not recopy it" >&2
      return 0
    fi
    echo "tools/cargo.sh: the main checkout has no client build: client-serving tests need 'cd app && npm run build'" >&2
  else
    echo "tools/cargo.sh: app/ differs from the main checkout's HEAD: client-serving tests need 'cd app && npm run build'" >&2
  fi
}

if [[ ${1:-} == test ]]; then
  copy_client
fi
heavy nice -n 2 cargo "$@"
exit $?
