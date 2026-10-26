# Sourced by tools/check.sh (the step pipeline-mod, .claude/rules/tools.md,
# "Pipeline mod"). The types of the engine of the mod,
# pipeline/.claude-plugin/types, are kept out of the repository: Claude
# Code lays them beside the mod when it loads the mod from the main checkout, so
# a linked worktree (agents, .claude/worktrees/check-pr-<n>) has none. There they
# are copied from the main checkout on every run, so that the mod is checked
# against the engine types the main checkout has now (a reused check-pr worktree
# would otherwise keep an old copy). The copy is a copy, not a link
# (tsconfig.json resolves its paths against itself), marked with the file
# .copied-from-main, and ignored by git: the folder holds its own .gitignore with
# `*`. A folder without the mark was laid by Claude Code in that checkout and is
# never touched. tsc is the client's (app/node_modules): this checkout's, else
# the main checkout's (a fresh worktree has none until npm ci), else PATH.

PIPELINE_TYPES=pipeline/.claude-plugin/types
# Where a main checkout that predates the move of the mod has them; to be dropped
# once every main checkout is past it.
PIPELINE_TYPES_OLD=tools/pipeline/.claude-plugin/types
PIPELINE_TYPES_MARK=.copied-from-main

# pipeline_main_root: the main checkout, also when called from a linked worktree
# (the same expression as tools/lib/worktree-target.sh).
pipeline_main_root() {
  dirname "$(git rev-parse --path-format=absolute --git-common-dir)"
}

# pipeline_linked: true in a linked worktree, false in the main checkout (the
# same test as tools/lib/worktree-target.sh).
pipeline_linked() {
  [[ $(git rev-parse --path-format=absolute --git-dir) != "$(git rev-parse --path-format=absolute --git-common-dir)" ]]
}

# lay_pipeline_types: in a linked worktree, the types are a fresh copy of the
# main checkout's on every run. A copy we made (marked) is removed and copied
# again, or just removed when the main checkout has no types now. Types without
# the mark, laid by Claude Code in this checkout, are left alone. The main
# checkout is never touched. Prints a line when it copies; fails when a step fails.
lay_pipeline_types() {
  local main src
  main=$(pipeline_main_root) || return 1
  pipeline_linked || return 0
  if [[ -e $PIPELINE_TYPES && ! -f $PIPELINE_TYPES/$PIPELINE_TYPES_MARK ]]; then
    return 0
  fi
  rm -rf "$PIPELINE_TYPES" || return 1
  src=$main/$PIPELINE_TYPES
  [[ -f $src/tsconfig.json ]] || src=$main/$PIPELINE_TYPES_OLD
  [[ -f $src/tsconfig.json ]] || return 0
  mkdir -p "$(dirname "$PIPELINE_TYPES")" || return 1
  cp -R "$src" "$PIPELINE_TYPES" || return 1
  : >"$PIPELINE_TYPES/$PIPELINE_TYPES_MARK" || return 1
  echo "the types of the engine are copied from $src (ignored by git)"
}

# pipeline_tsc: the path of tsc, empty when there is none.
pipeline_tsc() {
  local main t
  main=$(pipeline_main_root) || return 0
  for t in app/node_modules/.bin/tsc "$main/app/node_modules/.bin/tsc"; do
    if [[ -x $t ]]; then
      echo "$t"
      return 0
    fi
  done
  command -v tsc || true
}
