# Sourced by tools/check.sh: keeps the shared build dir (worktree-target.sh)
# from growing without limit. Both functions delete from the dir, so call them
# only under the lock of heavy work (`heavy limit_target`): no check, install
# or test-server build of ours runs then. A `cargo` started by hand takes no
# lock of ours; prune_target skips a profile while cargo holds its build lock,
# and `cargo clean` waits for that lock itself. Needs build-env.sh (fs_gb).

# target_warn <text>: a limit that cannot be kept - loud, on stderr. The run goes on.
target_warn() {
  echo "WARNING: $*" >&2
}

# workspace_crates: the names of the workspace packages joined with |, from
# `cargo metadata` (python3 reads the JSON: no jq needed).
workspace_crates() {
  local meta
  meta=$(cargo metadata --no-deps --format-version 1) || return 1
  printf '%s' "$meta" | python3 -c 'import json, sys; print("|".join(p["name"] for p in json.load(sys.stdin)["packages"]))'
}

# prune_target <dir>: remove the units of our own crates that cargo has not
# rebuilt for TARGET_PRUNE_DAYS (default 3; a unit that is still needed costs
# one rebuild of that crate). Cargo never removes old variants of
# them (a feature set, a profile, a changed Cargo.toml), and test binaries are
# hundreds of MB each. Dependencies (the slow part of a build) stay.
# Every unit has a fingerprint dir `.fingerprint/<package>-<hash>` whose
# `invoked.timestamp` cargo rewrites when it compiles the unit (a fresh unit
# keeps its old time); the hash names the unit's files in `deps/` and its dir
# in `build/`. Rustc's incremental state (`incremental/`, path crates only)
# goes by its dir time.
# TARGET_PRUNE_DRY=1 only prints what would go. Without the list of the
# workspace crates (no python3, or cargo metadata fails) nothing is pruned.
prune_target() {
  local dir=$1 days=${TARGET_PRUNE_DAYS:-3} pkgs profile
  if ! pkgs=$(workspace_crates) || [[ -z $pkgs ]]; then
    target_warn "not pruned: $dir keeps the units of the old crates (cargo metadata and python3 are needed); the size limit still applies"
    return 1
  fi
  for profile in "$dir"/*/; do
    if [[ -d $profile.fingerprint ]]; then prune_profile "${profile%/}" "$days" "$pkgs"; fi
  done
}

# prune_profile <profile dir> <days> <package names joined with |>
prune_profile() {
  local profile=$1 days=$2 pkgs=$3 lock fd unit hash p mb paths=() found=() incr=()
  lock=$profile/.cargo-build-lock
  [[ -e $lock ]] || lock=$profile/.cargo-lock
  # Cargo holds this lock for a build; ours until the removal is done.
  exec {fd}>>"$lock"
  if ! flock -n "$fd"; then
    exec {fd}>&-
    echo "prune: cargo is building in $profile, skipped"
    return 0
  fi
  while IFS= read -r unit; do
    hash=${unit: -16}
    paths+=("$profile/.fingerprint/$unit" "$profile/build/$unit" "$profile"/deps/*-"$hash"*)
  done < <(find "$profile/.fingerprint" -maxdepth 2 -name invoked.timestamp -mmin +$((days * 1440)) \
    -printf '%h\n' | sed 's|.*/||' | grep -E "^($pkgs)-[0-9a-f]{16}$")
  mapfile -t incr < <(find "$profile/incremental" -mindepth 1 -maxdepth 1 -type d -mmin +$((days * 1440)) 2>/dev/null)
  paths+=("${incr[@]}")
  for p in "${paths[@]}"; do [[ -e $p ]] && found+=("$p"); done
  if (( ${#found[@]} == 0 )); then
    echo "prune: nothing of the workspace crates older than $days days in $profile"
  else
    mb=$(du -sc --block-size=1M "${found[@]}" | tail -n 1 | cut -f1)
    if [[ -n ${TARGET_PRUNE_DRY:-} ]]; then
      echo "prune (dry run): $mb MB in ${#found[@]} paths of the workspace crates older than $days days in $profile"
      printf '  %s\n' "${found[@]}"
    else
      rm -rf "${found[@]}"
      echo "prune: removed $mb MB of the workspace crates unused for $days days from $profile"
    fi
  fi
  exec {fd}>&-
}

# target_limit_gb <free GB> <size GB>: the limit of the dir. It may take half
# of what it could hold (its size plus the free space of its disk), at most
# 30 GB: on a disk with little free space the limit is lower.
target_limit_gb() {
  local limit=$(( ($1 + $2) / 2 ))
  echo $(( limit < 30 ? limit : 30 ))
}

# limit_target: prune, then over the limit - cargo clean (a full build
# afterwards). The limit is TARGET_LIMIT_GB, or target_limit_gb. Only the
# shared dir; someone else's CARGO_TARGET_DIR is left alone. A pruning that
# cannot run does not skip the size limit; a limit that cannot be applied is a
# warning.
limit_target() {
  local dir=${CARGO_TARGET_DIR:-$SHARED_TARGET} size free limit
  [[ $dir == "$SHARED_TARGET" && -d $dir ]] || return 0
  prune_target "$dir" || true
  size=$(du -s --block-size=1G "$dir" | cut -f1)
  if [[ ! $size =~ ^[0-9]+$ ]]; then
    target_warn "cannot measure $dir: its size limit is NOT enforced"
    return 1
  fi
  if [[ -n ${TARGET_LIMIT_GB:-} ]]; then
    limit=$TARGET_LIMIT_GB
  else
    free=$(fs_gb 4 "$dir" 2>/dev/null) || free=
    if [[ $free =~ ^[0-9]+$ ]]; then
      limit=$(target_limit_gb "$free" "$size")
      if (( limit < 30 )); then
        echo "target limit: $limit GB, below the default 30 GB: the disk has $free GB free"
      fi
    else
      target_warn "cannot read the free space of the disk of $dir: the limit is the default 30 GB"
      limit=30
    fi
  fi
  if (( size > limit )); then
    echo "$dir is $size GB, over the limit of $limit GB: cargo clean"
    if [[ -z ${TARGET_PRUNE_DRY:-} ]] && ! cargo clean --target-dir "$dir"; then
      target_warn "cargo clean of $dir failed: it stays at $size GB, over the limit of $limit GB"
      return 1
    fi
  fi
}
