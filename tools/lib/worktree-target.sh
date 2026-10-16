# Sourced by tools/*.sh. One build dir for the main checkout and every linked
# worktree (agents, .claude/worktrees/): the root checkout's `target/`. A build
# from scratch is ~8 min and tens of GB, and the dependencies are the same, so
# a fresh worktree reuses what is already built (only our own crates are
# rebuilt there). Sets SHARED_TARGET (absolute) and, in a linked worktree with
# CARGO_TARGET_DIR unset, exports CARGO_TARGET_DIR=$SHARED_TARGET; the main
# checkout keeps the default (the same dir). An own CARGO_TARGET_DIR is kept.
# Pruning and the size limit: tools/lib/target-limit.sh.
SHARED_TARGET=$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")/target
if [[ -z ${CARGO_TARGET_DIR:-} && $(git rev-parse --git-dir) != "$(git rev-parse --git-common-dir)" ]]; then
  export CARGO_TARGET_DIR=$SHARED_TARGET
fi
