# Sourced by tools/*.sh. A linked worktree (agents, .claude/worktrees/) builds
# into one target dir shared by all worktrees: a build from scratch is ~8 min
# and tens of GB, the dependencies are the same. The orchestrator clears it
# between requests. The main checkout keeps its own target/.
if [[ -z ${CARGO_TARGET_DIR:-} && $(git rev-parse --git-dir) != "$(git rev-parse --git-common-dir)" ]]; then
  export CARGO_TARGET_DIR=${XDG_CACHE_HOME:-$HOME/.cache}/baluk-notes/worktree-target
fi
