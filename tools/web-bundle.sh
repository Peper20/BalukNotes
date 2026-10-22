#!/usr/bin/env bash
# Builds the browser version for the storage server on the computer: a folder
# that `notes hub serve --web <dir>` serves as it is (docs/architecture.md, 9;
# docs/server.md). The VPS has no Typst, so everything that needs it is made
# here.
#
#   tools/web-bundle.sh <out-dir> [--reuse-dist] [--reuse-wasm <dir>]
#
# <out-dir> is created or replaced (it must be absent, empty or an earlier
# bundle). --reuse-dist keeps an existing app/dist instead of building the
# client again. --reuse-wasm takes the core module and the packages from <dir>
# (what the `wasm-module` step of check.sh made in tests/.data/wasm: pkg/,
# fonts.json, fonts/ and files.json) instead of building them again.
#
# The folder (the list of files is the same for the same sources; it is not
# committed):
#   index.html, assets/...        the client (app/dist)
#   assets/config.js              the core in WASM on, syncing with the hub: { core: { module, packages, fonts, hub } }
#   wasm/notes_wasm.js, wasm/notes_wasm_bg.wasm    the core module (crates/notes-wasm/harness/build.sh)
#   wasm/fonts.json, wasm/fonts/font-<hash>.bin    the fonts of the core module, which carries none (the worker fetches them)
#   wasm/packages.json            { packages: [[path, base64]] }, the whitelisted Typst packages
#   api/fonts.css, api/themes.css, fonts/<family>/<style>/<part>.woff2
#                                 what `notes serve` answers for them, taken from a
#                                 temporary one over an empty data directory
#
# Needs: node and npm (the client, the packages), cargo with the target
# wasm32-unknown-unknown and wasm-bindgen-cli (build.sh says how to install
# them), curl, and the Typst packages of the whitelist in the Typst cache
# (`notes check` on a note that imports them downloads them).
set -euo pipefail

usage() {
  echo "usage: tools/web-bundle.sh <out-dir> [--reuse-dist] [--reuse-wasm <dir>]" >&2
  exit 2
}

out=""
reuse_dist=0
reuse_wasm=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --reuse-dist) reuse_dist=1; shift ;;
    --reuse-wasm) [[ $# -ge 2 ]] || usage; reuse_wasm=$2; shift 2 ;;
    -*) usage ;;
    *) [[ -z $out ]] || usage; out=$1; shift ;;
  esac
done
[[ -n $out ]] || usage

cd "$(dirname "$0")/.."
root=$PWD

fail() {
  echo "web-bundle: $*" >&2
  exit 1
}

# The folder is replaced: only an absent, empty or earlier-bundle folder is.
if [[ -e $out ]]; then
  [[ -d $out ]] || fail "$out is not a folder"
  if [[ -n $(ls -A "$out") && ! ( -f $out/index.html && -f $out/assets/config.js ) ]]; then
    fail "$out is not empty and not a bundle: give a new folder"
  fi
fi

if [[ -n $reuse_wasm ]]; then
  for f in pkg/notes_wasm.js pkg/notes_wasm_bg.wasm fonts.json files.json; do
    [[ -f $reuse_wasm/$f ]] || fail "--reuse-wasm: $reuse_wasm has no $f"
  done
  reuse_wasm=$(cd "$reuse_wasm" && pwd)
fi

for tool in node npm cargo curl; do
  command -v "$tool" >/dev/null || fail "$tool not found: install it first"
done

out=$(mkdir -p "$(dirname "$out")" && cd "$(dirname "$out")" && pwd)/$(basename "$out")
work=$(mktemp -d)
server_pid=""
cleanup() {
  [[ -z $server_pid ]] || kill "$server_pid" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT
stage=$work/bundle
mkdir -p "$stage"

echo "== client"
if [[ $reuse_dist == 1 && -f app/dist/index.html ]]; then
  echo "reusing app/dist"
else
  [[ -d app/node_modules ]] || (cd app && npm ci)
  (cd app && npm run -s build)
fi
[[ -f app/dist/index.html ]] || fail "app/dist has no index.html: the client did not build"
cp -R app/dist/. "$stage/"

echo "== core module"
# shellcheck source=lib/worktree-target.sh
source tools/lib/worktree-target.sh
# shellcheck source=lib/cpu-cap.sh
source tools/lib/cpu-cap.sh
if [[ -n $reuse_wasm ]]; then
  echo "reusing $reuse_wasm"
  wasm=$reuse_wasm
else
  wasm=$work/wasm
  OUT=$wasm heavy crates/notes-wasm/harness/build.sh >/dev/null
fi
mkdir -p "$stage/wasm"
for f in notes_wasm.js notes_wasm_bg.wasm; do
  [[ -f $wasm/pkg/$f ]] || fail "the module build left no $f"
  cp "$wasm/pkg/$f" "$stage/wasm/$f"
done
# The fonts of the module: the list and the files it names (paths relative to the list).
[[ -f $wasm/fonts.json && -d $wasm/fonts ]] || fail "the module build left no fonts.json and fonts/"
cp "$wasm/fonts.json" "$stage/wasm/fonts.json"
cp -R "$wasm/fonts" "$stage/wasm/fonts"

echo "== packages"
# The packages are the whitelist of run.mjs, in the form of the worker's seed.
if [[ -z $reuse_wasm ]]; then
  OUT=$wasm BOOK_REPEAT=0 node crates/notes-wasm/harness/run.mjs --prepare
fi
node -e '
  const fs = require("fs");
  const seed = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
  seed.packages.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  fs.writeFileSync(process.argv[2], JSON.stringify({ packages: seed.packages }));
' "$wasm/files.json" "$stage/wasm/packages.json"

echo "== stylesheets and fonts"
tools/cargo.sh build -q -p notes-typst
server=${CARGO_TARGET_DIR:-$SHARED_TARGET}/debug/notes-typst
[[ -x $server ]] || fail "no $server after the build"
port=$(node -e 'const s = require("net").createServer().listen(0, "127.0.0.1", () => { console.log(s.address().port); s.close(); })')
mkdir -p "$work/data"
"$server" --data "$work/data" serve --addr "127.0.0.1:$port" >"$work/server.log" 2>&1 &
server_pid=$!
base=http://127.0.0.1:$port
for _ in $(seq 1 300); do
  curl -fs -o /dev/null "$base/api/themes.css" && break
  kill -0 "$server_pid" 2>/dev/null || { cat "$work/server.log" >&2; fail "the temporary server stopped"; }
  sleep 0.2
done
mkdir -p "$stage/api"
curl -fs "$base/api/fonts.css" -o "$stage/api/fonts.css" || fail "$base/api/fonts.css did not answer"
curl -fs "$base/api/themes.css" -o "$stage/api/themes.css" || fail "$base/api/themes.css did not answer"
# Every part the stylesheet names: url("/fonts/<family>/<style>/<part>.woff2").
parts=$(grep -o 'url("[^"]*")' "$stage/api/fonts.css" | sed 's/^url("//; s/")$//' | sort -u)
[[ -n $parts ]] || fail "fonts.css names no font parts"
count=0
while IFS= read -r url; do
  [[ $url == /fonts/* && $url != *..* ]] || fail "unexpected font address in fonts.css: $url"
  rel=$(printf '%b' "${url//%/\\x}")
  mkdir -p "$stage/$(dirname "${rel#/}")"
  curl -fs "$base$url" -o "$stage/${rel#/}" || fail "$base$url did not answer"
  count=$((count + 1))
done <<<"$parts"
kill "$server_pid" 2>/dev/null || true
wait "$server_pid" 2>/dev/null || true
server_pid=""

cat >"$stage/assets/config.js" <<'JS'
// The browser version served by the hub: the core runs in a Web Worker
// (tools/web-bundle.sh).
globalThis.__NOTES_API__ = { core: { module: "/wasm/notes_wasm.js", packages: "/wasm/packages.json", fonts: "/wasm/fonts.json", hub: "" } };
JS

# In place of the old folder (the stage is on another file system maybe: copy).
rm -rf "$out"
mkdir -p "$out"
cp -R "$stage/." "$out/"
echo "== done: $out ($(find "$out" -type f | wc -l) files, $count font parts, $(du -sh "$out" | cut -f1))"
