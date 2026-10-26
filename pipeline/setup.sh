#!/bin/bash
# Installs the kit into a project: Claude Code finds agents in <project>/.claude/agents and skills in
# <project>/.claude/skills under their plain names, so each agent file and skill directory of the kit
# (claude/agents, claude/skills: not agents/ and skills/, which Claude Code would also load from the
# plugin root under the plugin's name, twice) gets a relative symbolic link there. Idempotent. Refuses, changing nothing, when a place is taken by
# something that is not a link to the kit's file.
#
#   pipeline/setup.sh [<project root>]     the root defaults to the git top level of the current directory
#
# The mod itself is loaded with `claude --plugin-dir <kit>`; the project layer is .claude/pipeline.json
# (config.md).
set -euo pipefail

kit=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
root=${1:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}
root=$(cd "$root" && pwd -P)

declare -a links=() targets=()
for src in "$kit"/claude/agents/*.md "$kit"/claude/skills/*/; do
  src=${src%/}
  [[ -e $src ]] || continue
  case $src in
    "$kit"/claude/agents/*) dir=$root/.claude/agents ;;
    *) dir=$root/.claude/skills ;;
  esac
  links+=("$dir/$(basename "$src")")
  targets+=("$(realpath -m --relative-to="$dir" "$src")")
done

refused=0
for i in "${!links[@]}"; do
  link=${links[$i]} want=${targets[$i]}
  if [[ -L $link ]]; then
    [[ $(readlink "$link") == "$want" ]] && continue
    echo "setup: $link is a link to $(readlink "$link"), not to the kit ($want)" >&2
    refused=1
  elif [[ -e $link ]]; then
    echo "setup: $link exists and is not a link: move or remove it first" >&2
    refused=1
  fi
done
((refused == 0)) || exit 1

for i in "${!links[@]}"; do
  link=${links[$i]}
  if [[ -L $link ]]; then
    continue
  fi
  mkdir -p "$(dirname "$link")"
  ln -s "${targets[$i]}" "$link"
  echo "linked ${link#"$root"/} -> ${targets[$i]}"
done

if [[ ! -f $root/.claude/pipeline.json ]]; then
  echo "note: $root/.claude/pipeline.json is missing: the mod refuses to start tasks without it (see $kit/config.md)"
fi
