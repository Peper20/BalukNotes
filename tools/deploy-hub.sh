#!/bin/bash
# Updates the storage server on a VPS from the laptop: sends the sources of a
# commit over SSH, builds there, installs and restarts (docs/server.md,
# "Updating"). The laptop builds nothing. An interrupted run can be repeated.
#
#   tools/deploy-hub.sh <ssh-host> [--ref <git-ref>] [--jobs <n>]
#
# On the VPS (relative to the home of the SSH user): sources in
# baluk-notes/src, the build in baluk-notes/target (outside the sources: the
# next build is incremental). tools/server/baluk-notes-hub-install knows the
# build path.
set -euo pipefail

usage() {
    echo "usage: tools/deploy-hub.sh <ssh-host> [--ref <git-ref>] [--jobs <n>]" >&2
    exit 2
}

host=""
ref=HEAD
jobs=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --ref) [ "$#" -ge 2 ] || usage; ref=$2; shift 2 ;;
        --jobs) [ "$#" -ge 2 ] || usage; jobs=$2; shift 2 ;;
        -*) usage ;;
        *) [ -z "$host" ] || usage; host=$1; shift ;;
    esac
done
[ -n "$host" ] || usage
case "$jobs" in *[!0-9]*) usage ;; esac

cd "$(dirname "$0")/.."
commit=$(git rev-parse --verify --quiet "$ref^{commit}") || { echo "no such commit: $ref" >&2; exit 1; }

echo "deploying $(git log -1 --format='%h %s' "$commit") to $host"
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
    echo "warning: the working tree has uncommitted changes; they are NOT deployed (only commit ${commit:0:7})" >&2
fi

# A long quiet build must not drop the session.
ssh=(ssh -o ServerAliveInterval=30 -o ServerAliveCountMax=20)
fail() { echo "deploy failed at step: $1" >&2; exit 1; }

echo "== sending sources"
git archive --format=tar "$commit" |
    "${ssh[@]}" "$host" 'set -e; mkdir -p baluk-notes; cd baluk-notes; rm -rf src.new; mkdir src.new; tar -x -C src.new; rm -rf src; mv src.new src' ||
    fail "sending sources"

echo "== building on $host (the first build takes a while)"
"${ssh[@]}" "$host" bash -s -- "${jobs:-0}" <<'REMOTE' || fail "build (the running server is untouched)"
set -euo pipefail
[ -r "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
command -v cargo >/dev/null || { echo "cargo not found: install rustup for this user (docs/server.md)" >&2; exit 1; }
cd "$HOME/baluk-notes/src"
need=$(sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml | head -1)
have=$(rustc --version | cut -d' ' -f2)
if [ "$(printf '%s\n%s\n' "$need" "$have" | sort -V | head -1)" != "$need" ]; then
    echo "Rust $have is too old, need $need: run 'rustup update stable' on the VPS" >&2
    exit 1
fi
# The first build needs ~1 GB (toolchain aside); keep a margin.
free_mb=$(df -Pm "$HOME" | awk 'NR == 2 { print $4 }')
if [ "$free_mb" -lt 1500 ]; then
    echo "only $free_mb MB free in $HOME, need 1500: free some space (cargo cache: ~/.cargo/registry)" >&2
    exit 1
fi
args=()
[ "$1" = 0 ] || args=(--jobs "$1")
export CARGO_TARGET_DIR="$HOME/baluk-notes/target"
# The site on the same machine stays responsive: the lowest CPU and disk priority.
low=(nice -n 19)
command -v ionice >/dev/null && low=(ionice -c3 nice -n 19)
"${low[@]}" cargo build --release --locked -p notes -p notes-hub --no-default-features "${args[@]}"
REMOTE

echo "== installing and restarting"
"${ssh[@]}" "$host" 'sudo -n /usr/local/sbin/baluk-notes-hub-install' ||
    fail "install (is the helper and its sudoers rule in place? docs/server.md)"

echo "== now running"
"${ssh[@]}" "$host" '/usr/local/bin/notes --version; echo "service: $(systemctl is-active baluk-notes-hub || true)"' ||
    fail "version check"
if ! "${ssh[@]}" "$host" 'systemctl is-active --quiet baluk-notes-hub'; then
    "${ssh[@]}" "$host" 'journalctl -u baluk-notes-hub -n 20 --no-pager' || true
    fail "the service is not active"
fi
