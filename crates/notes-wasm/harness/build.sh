#!/usr/bin/env bash
# Builds the E4 prototype for the browser (docs/research/E4.md): the release
# module and its JS glue into $OUT/pkg (default tests/.data/wasm/pkg, not in
# git), and the fonts the module does not carry: $OUT/fonts.json and
# $OUT/fonts/font-<hash>.bin (the `fonts` example; the host fetches them and
# hands them to `WasmCore.addFont`). Needs the target `wasm32-unknown-unknown`
# and `wasm-bindgen-cli` of exactly the version of the `wasm-bindgen` crate
# (both user-level: `rustup target add`, `cargo install`).
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/../../.." && pwd)
out=${OUT:-$root/tests/.data/wasm}
# The shared build dir of the worktrees (.claude/rules/tools.md, "Checks").
if [[ -z ${CARGO_TARGET_DIR:-} ]]; then
  # shellcheck source=../../../tools/lib/worktree-target.sh
  source "$root/tools/lib/worktree-target.sh"
fi
target=${CARGO_TARGET_DIR:-$root/target}

if ! rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
  echo "no Rust target wasm32-unknown-unknown: rustup target add wasm32-unknown-unknown" >&2
  exit 2
fi
want=$(sed -n 's/^wasm-bindgen = "=\(.*\)"$/\1/p' "$root/crates/notes-wasm/Cargo.toml")
bindgen=${WASM_BINDGEN:-$(command -v wasm-bindgen || echo "$HOME/.cargo/bin/wasm-bindgen")}
have=$("$bindgen" --version 2>/dev/null | awk '{print $2}' || true)
if [[ $have != "$want" ]]; then
  echo "wasm-bindgen-cli $want is needed (found: ${have:-none}): cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 2
fi

cd "$root"
cargo rustc -p notes-wasm --release --target wasm32-unknown-unknown --crate-type cdylib
mkdir -p "$out/pkg"
"$bindgen" --target web --remove-name-section --remove-producers-section --out-dir "$out/pkg" \
  "$target/wasm32-unknown-unknown/release/notes_wasm.wasm"
# The fonts are a native program (the same set the native binary embeds).
cargo run -q -p notes-wasm --example fonts -- "$out"
ls -l "$out/pkg"
