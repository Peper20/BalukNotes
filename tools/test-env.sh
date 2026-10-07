#!/usr/bin/env bash
# Test environment: a server on the fixture vault tests/vault with its own
# data (settings, cache) in tests/.data - separate from the user's data/.
#
#   tools/test-env.sh               # http://127.0.0.1:8432, debug build
#   tools/test-env.sh --fresh       # clear tests/.data first (default settings, empty cache)
#   tools/test-env.sh --release     # release build (embedded client and library)
#   PORT=8500 tools/test-env.sh
#
# Stop - Ctrl+C or `fuser -k -TERM 8432/tcp` (by port: `pgrep -x notes-typst`
# also finds the `notes serve` service).
set -euo pipefail
cd "$(dirname "$0")/.."

port="${PORT:-8432}"
profile=()
for arg in "$@"; do
  case "$arg" in
    --fresh) rm -rf tests/.data ;;   # test cache and settings - nothing to restore
    --release) profile=(--release) ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done
mkdir -p tests/.data

echo "test environment: http://127.0.0.1:$port  (vault tests/vault, data tests/.data)"
exec cargo run -q "${profile[@]}" -p notes-typst -- \
  --data tests/.data --vault tests/vault serve --addr "127.0.0.1:$port"
