# Sourced by tools/check.sh, install.sh, test-env.sh and cargo.sh, after
# worktree-target.sh: what a small disk and a cloud session change in the
# builds, and what is missing there (`.claude/rules/tools.md`, "Checks").
#
# A small disk is a file system under SMALL_DISK_GB (default 100) in total: its
# size decides, not its free space, so the setting does not flip from run to run.
# A cloud session is CLAUDE_CODE_REMOTE=true (set by Claude Code on the web) or
# BALUK_CLOUD=1. Both export CARGO_PROFILE_DEV_DEBUG=line-tables-only: the full
# debug info of our crates is tens of GB on a small disk. A value set by hand is
# kept. The profiles in Cargo.toml do not change (the dependencies already have
# line tables there), so the local machine keeps the full debug info.

SMALL_DISK_GB=${SMALL_DISK_GB:-100}

# fs_gb <column> <path>: a column of df for the file system of <path> (or of
# its nearest existing parent), in whole GB: 2 - size, 4 - available.
fs_gb() {
  local path=$2
  while [[ ! -e $path ]]; do path=$(dirname "$path"); done
  df -P --block-size=1G "$path" | awk -v col="$1" 'NR == 2 {print $col}'
}

# is_cloud: a cloud session (see above).
is_cloud() {
  [[ ${CLAUDE_CODE_REMOTE:-} == true || ${BALUK_CLOUD:-} == 1 ]]
}

# has_webkit: WebKitGTK 4.1 is installed, so the window (notes-app) can be
# built. Without it (a cloud session) the window is left out of the builds.
has_webkit() {
  command -v pkg-config >/dev/null && pkg-config --exists webkit2gtk-4.1
}

# The setting itself. Only the build dir of this checkout is probed.
if [[ -z ${CARGO_PROFILE_DEV_DEBUG:-} ]]; then
  reason=
  if is_cloud; then
    reason="cloud session"
  else
    total=$(fs_gb 2 "${CARGO_TARGET_DIR:-$PWD/target}" 2>/dev/null) || total=
    if [[ $total =~ ^[0-9]+$ ]] && (( total < SMALL_DISK_GB )); then
      reason="small disk, ${total} GB"
    fi
  fi
  if [[ -n $reason ]]; then
    export CARGO_PROFILE_DEV_DEBUG=line-tables-only
    echo "build: $reason - line tables only (CARGO_PROFILE_DEV_DEBUG)" >&2
  fi
  unset reason total
fi
