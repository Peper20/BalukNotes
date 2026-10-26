# Sourced by a project's steps file for the check of the kit's mod
# (`pipeline_mod_check`). The types of the engine of the mod,
# <kit>/.claude-plugin/types, are kept out of the repository: Claude Code lays
# them beside the mod when it loads the mod from the main checkout, so a linked
# worktree (agents, the worktree of a pull request's check) has none. There they
# are copied from the main checkout on every run, so that the mod is checked
# against the engine types the main checkout has now (a reused worktree would
# otherwise keep an old copy). The copy is a copy, not a link (tsconfig.json
# resolves its paths against itself), marked with the file .copied-from-main,
# and ignored by git: the folder holds its own .gitignore with `*`. A folder
# without the mark was laid by Claude Code in that checkout and is never
# touched.

source "$(dirname "${BASH_SOURCE[0]}")/config.sh"

# The kit against the project root, and where its engine types are.
PIPELINE_KIT=$(realpath -m --relative-to="$PROJECT_ROOT" "$KIT_DIR")
PIPELINE_TYPES=$PIPELINE_KIT/.claude-plugin/types
PIPELINE_TYPES_MARK=.copied-from-main

# pipeline_main_root: the main checkout, also when called from a linked worktree.
pipeline_main_root() {
  dirname "$(git rev-parse --path-format=absolute --git-common-dir)"
}

# pipeline_linked: true in a linked worktree, false in the main checkout.
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
  [[ -f $src/tsconfig.json ]] || return 0
  mkdir -p "$(dirname "$PIPELINE_TYPES")" || return 1
  cp -R "$src" "$PIPELINE_TYPES" || return 1
  : >"$PIPELINE_TYPES/$PIPELINE_TYPES_MARK" || return 1
  echo "the types of the engine are copied from $src (ignored by git)"
}

# pipeline_tsc <path>...: the first of the paths (relative to a checkout) of a
# `tsc` that exists in this checkout, else in the main checkout, else `tsc` of
# PATH; empty when there is none.
pipeline_tsc() {
  local main p t
  main=$(pipeline_main_root) || return 0
  for p in "$@"; do
    for t in "$p" "$main/$p"; do
      if [[ -x $t ]]; then
        echo "$t"
        return 0
      fi
    done
  done
  command -v tsc || true
}

# pipeline_mod_check <tsc path>...: the manifest and the module, its tests, and
# the types with tsc (the paths as `pipeline_tsc` takes them). Without the types
# (neither checkout has them) or without tsc the type check is left out with a
# line in the log.
pipeline_mod_check() {
  lay_pipeline_types || return 1
  claude plugin validate "$PIPELINE_KIT" || return 1
  claude plugin test "$PIPELINE_KIT" || return 1
  local tsc
  tsc=$(pipeline_tsc "$@")
  if [[ -z $tsc ]]; then
    echo "no tsc: the types of the mod were not checked"
  elif [[ ! -f $PIPELINE_TYPES/tsconfig.json ]]; then
    echo "the types of the engine are not laid here or in the main checkout (Claude Code lays them when it loads the mod): the types of the mod were not checked"
  else
    "$tsc" -p "$PIPELINE_KIT" || return 1
  fi
}
