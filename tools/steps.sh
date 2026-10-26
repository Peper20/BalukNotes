# The steps of the full check of this project (`check.steps` of
# .claude/pipeline.json), sourced by the kit's runner pipeline/scripts/check.sh,
# which gives `step`, `step_unlocked`, `skip`, `last_ok` and `$logs`
# (pipeline/docs/README.md, "Checks"). Run it as tools/check.sh:
#
#   tools/check.sh           # everything: Rust, the fixture vault, API types, client, e2e
#   tools/check.sh --fast    # without the client build and e2e (a minute or two)
#   tools/check.sh --rust    # Rust only: test, clippy, fmt, crate copies, notes check, note templates, API types
#   tools/check.sh --app     # client only: check, Vitest, build, e2e
#
# All steps run even if one fails (except dependent ones: e2e does not run
# without a client build). At the end - a table "step -> ok/failed, time";
# the output of a failed step is in tests/.data/check/<step>.log (its tail is printed).
# The exit code is non-zero if any step failed.

source tools/lib/worktree-target.sh
# The build settings of a small disk and a cloud session (CARGO_PROFILE_DEV_DEBUG).
source tools/lib/build-env.sh
# The types of the pipeline mod in a linked worktree (the step pipeline-mod).
source pipeline/scripts/lib/mod-check.sh
# The step cargo-test: the test binaries run at the same time.
source tools/lib/cargo-test.sh
# The shared build dir grows without limit: pruning and the size limit
# (tools/lib/target-limit.sh) run first, under the lock of heavy work.
source tools/lib/target-limit.sh
before_steps() { heavy limit_target; }

# notes check on tests/vault: exactly the intended problems. The expected total
# is the line "Total: `...`" in .claude/rules/tests-vault.md (the core test checks it too).
vault_check() {
  local expected actual
  expected=$(sed -n 's/^Total: `\(.*\)`$/\1/p' .claude/rules/tests-vault.md)
  if [[ -z $expected ]]; then
    echo "no line \"Total: \`...\`\" in .claude/rules/tests-vault.md"
    return 1
  fi
  # Exit code 1 is expected: the fixture has intended errors.
  actual=$(cargo run -q -p notes-typst -- --data tests/.data --vault tests/vault check | tee /dev/stderr | tail -n 1)
  echo "expected: $expected"
  echo "actual:   $actual"
  [[ $actual == "$expected" ]]
}

# API types are exported from Rust and committed: no changes after the export.
api_types() {
  local dir=app/src/lib/api/types
  (cd app && npm run -s types) || return 1
  local changed
  changed=$(git status --porcelain -- "$dir")
  if [[ -n $changed ]]; then
    echo "API types are stale - export them (cd app && npm run types) and commit:"
    echo "$changed"
    git --no-pager diff --stat -- "$dir"
    return 1
  fi
}

# Copies of foreign crates with patches (.claude/rules/vendor.md) - their own tests,
# on one thread: the comemo cache is global (the rule says why).
vendor_tests() {
  CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$PWD/target}/vendor cargo test -q --manifest-path vendor/comemo/Cargo.toml --features testing -- --test-threads=1 || return 1
  rm -f vendor/comemo/Cargo.lock
}

# The /baluk-note skill: `notes new` templates (a note and a book; the file
# name comes from a title with markup characters and characters forbidden in
# file names) and the examples skills/baluk-note/examples build in a temporary
# vault without errors, warnings or broken links.
baluk_note() {
  local dir=tests/.data/baluk-note notes=(cargo run -q -p notes-typst -- --data tests/.data/baluk-note)
  rm -rf "$dir"
  # `vaults` is a light command: the thin notes runs it, notes-typst does not know it.
  cargo run -q -p notes -- --data "$dir" vaults new Проверка >/dev/null || return 1
  notes+=(--vault Проверка)
  "${notes[@]}" new --tag проверка "Математика/Заметка" || return 1
  "${notes[@]}" new --book --lang en --title 'C++ [1] #x $y$ // - z' "Книга" || return 1
  "${notes[@]}" new --folder Сеть --title 'SSH: основы? "1.0"... <черновик> 1. -- a/b @x' || return 1
  [[ $("${notes[@]}" new --folder Сеть --title '/\:*?"<>|' | tail -n 1) == "Сеть/Без названия" ]] || return 1
  local vault=$dir/vaults/Проверка
  cp -r skills/baluk-note/examples "$vault/examples"
  local actual
  actual=$("${notes[@]}" check | tee /dev/stderr | tail -n 1)
  [[ $actual == "notes: 9, errors: 0, warnings: 0, broken links: 0" ]]
}

in_app() { (cd app && "$@"); }

# The core builds for the browser (docs/research/E4.md): notes-wasm and with it
# notes-core and notes-store for wasm32-unknown-unknown, lints included.
wasm_check() {
  if ! rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
    echo "no Rust target wasm32-unknown-unknown - install it: rustup target add wasm32-unknown-unknown"
    return 1
  fi
  cargo clippy -p notes-wasm --target wasm32-unknown-unknown -- -D warnings
}

# The release module of the core and the vault files for the browser e2e
# (app/e2e/wasm.spec.ts reads tests/.data/wasm): the wasm-bindgen glue and
# files.json, as the E4 harness makes them.
wasm_module() {
  crates/notes-wasm/harness/build.sh || return 1
  node crates/notes-wasm/harness/run.mjs --prepare
}

# The pipeline mod (pipeline/, a Claude Code plugin; .claude/rules/tools.md,
# "Pipeline mod"): the kit's check (pipeline/scripts/lib/mod-check.sh) with the
# client's tsc.
pipeline_mod() {
  pipeline_mod_check app/node_modules/.bin/tsc
}

steps() {
  local rust=1 app=1 fast=0 arg
  for arg in "$@"; do
    case "$arg" in
      --fast) fast=1 ;;
      --rust) app=0 ;;
      --app) rust=0 ;;
      -h|--help) sed -n '6,14p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
      *) echo "unknown option: $arg (known: --fast, --rust, --app)" >&2; exit 2 ;;
    esac
  done
  if [[ $rust == 0 && $app == 0 ]]; then
    echo "--rust and --app together is just tools/check.sh" >&2
    exit 2
  fi

  # The notes-app window (Tauri) - only with WebKitGTK; without it (cloud) - skipped.
  local no_webkit=()
  if ! has_webkit; then
    no_webkit=(--exclude notes-app)
    echo "no webkit2gtk-4.1 - notes-app is not built"
  fi

  # The shell libraries of tools/lib (target limit, build settings, lock): no cargo build.
  step tools-libs tools/test-libs.sh
  if claude plugin test --help >/dev/null 2>&1; then
    step pipeline-mod pipeline_mod
  else
    echo "no claude with plugin tests - the pipeline mod is not checked"
    skip pipeline-mod "no claude plugin"
  fi

  if [[ $rust == 1 ]]; then
    # Server tests serve the client from app/dist (a debug build reads it from
    # disk): a fresh checkout (a worktree) builds the client first.
    if [[ ! -f app/dist/index.html ]]; then
      [[ -d app/node_modules ]] || step npm-ci in_app npm ci
      step client-dist in_app npm run -s build
    fi
    # The binaries run at the same time within RUST_TEST_THREADS (lib/cargo-test.sh);
    # the log of each is in $logs/cargo-test/, the step log holds them in turn.
    step cargo-test cargo_test "$logs/cargo-test" --workspace "${no_webkit[@]}"
    # With the measure feature - also the heavy memory measurements (they do not run as tests).
    step clippy cargo clippy --workspace "${no_webkit[@]}" --all-targets --features notes-core/measure,notes-wasm/measure -- -D warnings
    step wasm wasm_check
    step fmt cargo fmt --check
    step vendor vendor_tests
    step notes-check vault_check
    step baluk-note baluk_note
    step api-types api_types
  fi

  # After the Rust steps (the Typst packages for files.json are downloaded by
  # them) and before the e2e that uses the module.
  if [[ $app == 1 && $fast == 0 ]]; then
    step wasm-module wasm_module
  fi

  if [[ $app == 1 ]]; then
    if [[ ! -d app/node_modules ]]; then
      step npm-ci in_app npm ci
    fi
    step svelte-check in_app npm run -s check
    step vitest in_app npm test
    if [[ $fast == 1 ]]; then
      skip build "--fast"
      skip e2e "--fast"
    else
      step build in_app npm run -s build
      if last_ok; then
        # The browser version for the hub e2e (app/e2e/hub.spec.ts): the client just
        # built and the module of wasm-module, in tests/.data/web.
        step_unlocked web-bundle tools/web-bundle.sh tests/.data/web --reuse-dist --reuse-wasm tests/.data/wasm
        if last_ok; then
          # e2e listens on fixed ports (8600-8649, app/e2e/ports.ts): runs in
          # parallel worktrees must not overlap - the lock of the step takes care of it.
          step e2e in_app npm run -s e2e
        else
          skip e2e "the browser version did not build"
        fi
      else
        skip e2e "the client did not build"
      fi
    fi
  fi
}
