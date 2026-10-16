# Sourced by tools/check.sh: keeps the shared build dir (worktree-target.sh)
# from growing without limit. Both functions delete from the dir, so call them
# only under the lock of heavy work (`heavy limit_target`): no check, install
# or test-server build of ours runs then. A `cargo` started by hand takes no
# lock of ours; prune_target skips a profile while cargo holds its build lock,
# and `cargo clean` waits for that lock itself.

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
# TARGET_PRUNE_DRY=1 only prints what would go.
prune_target() {
  local dir=$1 days=${TARGET_PRUNE_DAYS:-3} pkgs profile
  pkgs=$(cargo metadata --no-deps --format-version 1 | jq -r '[.packages[].name] | join("|")') || return 1
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

# limit_target: prune, then over TARGET_LIMIT_GB (default 30) - cargo clean
# (a full build afterwards). Only the shared dir; someone else's
# CARGO_TARGET_DIR is left alone.
limit_target() {
  local limit=${TARGET_LIMIT_GB:-30} dir=${CARGO_TARGET_DIR:-$SHARED_TARGET} size
  [[ $dir == "$SHARED_TARGET" && -d $dir ]] || return 0
  prune_target "$dir" || return 1
  size=$(du -s --block-size=1G "$dir" | cut -f1)
  if (( size > limit )); then
    echo "$dir is $size GB, over the limit of $limit GB: cargo clean"
    [[ -n ${TARGET_PRUNE_DRY:-} ]] || cargo clean --target-dir "$dir"
  fi
}
