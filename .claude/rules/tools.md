---
paths:
  - "tools/**"
  - "pipeline/**"
---

# tools - checks, screenshots, installation

| File | What |
|---|---|
| `check.sh` | the full check: the entry point of the kit's runner `pipeline/scripts/check.sh` with the steps of `steps.sh` |
| `steps.sh` | the steps of this project's full check and its options `--fast/--rust/--app` (`check.steps` of `.claude/pipeline.json`) |
| `cargo.sh` | `cargo` by hand with the settings of the checks: the shared build dir, the CPU cap, the lock of heavy work and `freshen_sources` |
| `check-pr.sh` | a shim: `exec` of the kit's `pipeline/scripts/check-pr.sh` (the full check of a pull request, "CI"), kept for the runner installed before the kit (`docs/tech-debt.md`, "Pipeline") |
| `install.sh` | `notes` and its parts (`notes-typst`, `notes-hub`, the `notes-app` window with a menu entry and icon) into `~/.local/bin` and the skill into `~/.claude/skills` in one version (the autostart service is not installed; one still active is restarted); run again after changes |
| `deploy-hub.sh` | updates `notes-hub` on the VPS in one command (`--web`: also the browser version): sends the sources of a commit over SSH, the VPS builds, a root helper (`server/baluk-notes-hub-install`, installed by hand) installs and restarts (`docs/server.md`, "Updating") |
| `web-bundle.sh` | builds the browser version for the hub into a folder (`web-bundle.sh <dir> [--reuse-dist] [--reuse-wasm <dir>]`: the client or the module that a build already made): the client, the core module, the packages, the stylesheets and font parts from a temporary `notes-typst serve`; `notes hub serve --web <dir>` serves it (architecture §9, `docs/server.md`); `deploy-hub.sh --web` sends it |
| `test-env.sh` | a server on `tests/vault` (data `tests/.data`, port 8432, another - `PORT=...`) |
| `lib/worktree-target.sh` | in a linked worktree (agents) - the build dir of the main checkout, `<root>/target` |
| `lib/target-limit.sh` | pruning and the size limit of that dir, sourced by `steps.sh` (the crate list by python3, no `jq`) |
| `lib/cpu-cap.sh` | a shim: sources the kit's `pipeline/scripts/lib/cap.sh` (the CPU cap `CHECK_JOBS` and the lock of heavy work) for `install.sh`, `test-env.sh`, `cargo.sh`, `web-bundle.sh` |
| `lib/cpu-env.sh` | the project's part of the cap (`check.env`): the settings of its tools from `CHECK_JOBS` ("CPU cap") and `freshen_sources` |
| `lib/build-env.sh` | the build settings of a small disk and a cloud session (`CARGO_PROFILE_DEV_DEBUG`) and the WebKitGTK check, sourced by `steps.sh`, `install.sh`, `test-env.sh`, `cargo.sh` |
| `lib/cargo-test.sh` | the step `cargo-test` of `steps.sh`: the test binaries run at the same time within `RUST_TEST_THREADS` (below, "CPU cap") |
| `test-libs.sh` | tests of the libraries above (the scheme of `lib/cargo-test.sh` with fake test binaries, the settings of `lib/cpu-env.sh`), of `cargo.sh` and the `check-pr.sh` shim, the refusals of `web-bundle.sh` and `pipeline/setup.sh`, without a cargo build - the first step of the full check; it runs the kit's `pipeline/scripts/test-scripts.sh` at the end |
| `visual.mjs`, `shot.mjs` | screenshots of fixtures and pages (browser - `lib/browser.mjs`) |
| `shot-webkit.py` | the same page in WebKitGTK (the engine of the app window): a screenshot or a measurement, `--dark`/`--light`, `--print 'JS'`; runs itself inside a headless gamescope |
| `app-smoke.sh` | a smoke launch of the `notes-app` window inside a headless gamescope: it opens, loads the fixture vault, quits cleanly (~6 s); not in `check.sh` (needs gamescope and an installed or built `notes-app`) |
| `../pipeline/` | the development cycle kit (the mod, the agents, the skills, the scripts of the checks, of CI and of the cloud Forgejo, its documents), in the repository root; `tools/pipeline` is a link to it ("Pipeline mod") |

## Checks

Tests do not touch the user's data: the fixture vault `tests/vault/` (the case
catalog - `.claude/rules/tests-vault.md`), test data - `tests/.data/` (not in
git, can be deleted).

```sh
tools/check.sh --fast   # no client build and e2e
                        # also --rust, --app
tools/check.sh          # full (~3 min warm); CI runs it on every pull request push
pipeline/scripts/test.sh   # the targeted tests of what you changed (`tests` of .claude/pipeline.json)
```

`check.sh` is the kit's runner (`pipeline/docs/README.md`, "Checks"): it calls
`steps` of `tools/steps.sh` (the steps and the options below are this
project's), runs every step under the lock of heavy work and writes the table,
`summary.md` and the logs to `tests/.data/check/`. `check.sh` and `install.sh`
run with lower than normal priority (`nice` 2 and 1, `ionice` 5): the computer
does not lag. Steps: cargo test, clippy (`-D warnings`), fmt; clippy of
`notes-wasm` for `wasm32-unknown-unknown` (the step `wasm`: fails with the
install command when the target is missing, `rustup target add
wasm32-unknown-unknown`; the step `wasm-module`, in the full run before e2e,
builds the release module with its fonts (`fonts.json`, `fonts/`) and
`files.json` into `tests/.data/wasm` for `app/e2e/wasm.spec.ts`; the step
`web-bundle`, after the client build and before e2e, runs `tools/web-bundle.sh
tests/.data/web --reuse-dist --reuse-wasm tests/.data/wasm` for
`app/e2e/hub.spec.ts`: it takes the lock of heavy work itself where it needs
it, not as a locked step); `notes check` on `tests/vault` (the total is
compared with the "Total" line in `.claude/rules/tests-vault.md`); `notes new`
templates and the skill (`.claude/rules/skills.md`); exported API types match
the committed ones (uncommitted ones also count as "failed"); the pipeline mod
(validate, tests, `tsc`, "Pipeline mod"); the client - check, Vitest, build,
e2e. At the end - a table of steps with times, a non-zero exit code on any
failure, logs in `tests/.data/check/`. e2e needs a system Chromium
(`/usr/bin/chromium`, another one - `CHROMIUM=...`). The `notes-app` window is
built if WebKitGTK is present (`pkg-config webkit2gtk-4.1`), otherwise it is
excluded from the cargo steps (cloud). In a linked worktree `check.sh` and
`test-env.sh` build into the `target/` of the main checkout
(`lib/worktree-target.sh`, an absolute `CARGO_TARGET_DIR`; your own value is
kept), so the dependencies built once serve every worktree; only our own
crates are rebuilt in a new worktree. Run `cargo` by hand as `tools/cargo.sh
<arguments>`: a plain `cargo` in a worktree builds from scratch into the
worktree's own `target/`. A worktree is inside the main checkout, so cargo
reads `.cargo/config.toml` twice there and the `rustflags` differ from a build
in the main checkout: that build makes its own copy of the dependencies. A
plain `cargo` in the main checkout takes no lock of ours; cargo's own
build-dir lock keeps it from overlapping a check (they wait for each other).
The unit hash of our crates does not contain the checkout path and cargo
judges them fresh by file times, so a build of another checkout, newer than
your files, would be taken for yours. The lock of heavy work (below) records
the checkout that held it last: the first holder after a run of another
checkout (`check.sh` steps, `install.sh`, `test-env.sh`) sets the times of the
tracked sources to now (`freshen_sources` in `lib/cpu-env.sh`, called by the
hook `on_heavy_acquire`), and cargo recompiles our crates against this tree. A
repeated run of the same checkout does not, so our crates are not rebuilt.
Dependencies and the foreign copies in `vendor/` stay built (after an edit
under `vendor/`, `touch` the file or `cargo clean -p` that crate).
`tools/cargo.sh` takes the same lock. A plain `cargo` by hand is exposed to
this and leaves the record stale, so the next run of the recorded checkout
reuses its artifacts. Before the first step (`before_steps` of `steps.sh`),
under the lock of heavy work, `lib/target-limit.sh` keeps the dir bounded
(cargo removes nothing itself): units of our own crates that cargo has not
rebuilt for `TARGET_PRUNE_DAYS` (3) are removed (`TARGET_PRUNE_DRY=1` - only
list them), dependencies stay. The list of the crates comes from `cargo
metadata`, read by python3 (no `jq` needed); without python3 the pruning is
skipped with a `WARNING` and the size limit still applies. Then over the limit
- `cargo clean`. The limit is `TARGET_LIMIT_GB` or, by default, half of what
the dir could hold (its size plus the free space of its disk), at most 30 GB:
on a disk with little free space it is lower, and a line `target limit:` says
so. A limit that cannot be measured or cleaned is a `WARNING` on stderr. Both
skip someone else's `CARGO_TARGET_DIR`; the pruning skips a profile while
cargo holds its build lock. The vendored `comemo` tests build into `vendor/`
of the same dir. `lib/build-env.sh` (sourced after the cap) sets
`CARGO_PROFILE_DEV_DEBUG=line-tables-only` (not in `Cargo.toml`) when the file
system of the build dir is under `SMALL_DISK_GB` (100) in total, or in a cloud
session (`CLAUDE_CODE_REMOTE=true` or `BALUK_CLOUD=1`); a value set by hand is
kept. It also answers whether WebKitGTK is installed (`has_webkit`).
`tools/test-libs.sh`, the first step of `check.sh`, tests these libraries and
`tools/cargo.sh` without a cargo build, with shims for `jq`, `df`, `python3`,
`pkg-config` and `cargo`; then it runs the kit's
`pipeline/scripts/test-scripts.sh`. The full check of a pull request is the
kit's `pipeline/scripts/check-pr.sh <number>` (run by CI, "CI" below; by hand
from any checkout): `app/dist` of an older head is removed from its worktree
(`ci.staleOutput` of `.claude/pipeline.json`), the logs are in
`tests/.data/check/` of the worktree (`check.sh` clears that folder at the
start; the whole output is `run.log` there). Without `app/dist` (a fresh
checkout) the client is built before the Rust steps - server tests serve it. e2e listens on fixed ports
(8600-8649, `app/e2e/ports.ts`): the lock of heavy work (below) keeps runs
from overlapping. The e2e servers keep their data in RAM (`XDG_RUNTIME_DIR` or
`/dev/shm`, else `tests/.data`; `DATA_ROOT` of `app/e2e/server.ts`, ~25 MB):
on this ext4 any small write (a settings PUT) waits up to ~12 s while the
gigabytes of the build steps are written out, past the 10 s of an `expect`.

## CI

The mechanism (the workflow, the runner, `PIPELINE_MAIN`, the statuses, the
logs, host execution) is the kit's: `pipeline/docs/README.md`, "Forgejo". This
project's side:

- The runner is `baluk-host` (`ci.runner` of `.claude/pipeline.json`) on this
  machine, installed by `pipeline/scripts/ci-runner.sh install`. `ci.tools`
  names what the check needs (`cargo node npm chromium`; the e2e step needs a
  system Chromium, another one - `CHROMIUM=...`).
- `.forgejo/workflows/check.yml` is the kit's template
  (`pipeline/forgejo/check.yml`) with `runs-on: baluk-host` and a transitional
  run block: it falls back to `BALUK_MAIN` of the runner installed before the
  kit and to the shim `tools/check-pr.sh` (`docs/tech-debt.md`, "Pipeline").
- The job logs of a run: `/ibaluk/baluk-notes/actions` on the server.

## CPU cap

`check.sh`, `install.sh`, `test-env.sh` (its build) and `cargo.sh` keep at most
`CHECK_JOBS` logical CPUs busy (default: all of a machine with 1-2 CPUs,
otherwise about five sixths: n - round(n/6), so 12 -> 10, 6 -> 5; never more
than the machine has),
also when several runs go at once (agents in worktrees). Priority stays as
above (`nice` 2 and 1); the cap is on top of it. The formula, the lock and the
hook files are the kit's (`pipeline/docs/README.md`, "Checks"); the lock is
`check.heavyLock` of `.claude/pipeline.json`.

- `lib/cpu-env.sh` turns the number into the setting of each tool:
  `CARGO_BUILD_JOBS` (one below `CHECK_JOBS`; overrides `jobs` of
  `~/.cargo/config.toml`), `RUST_TEST_THREADS` (one below: the test threads
  of all the binaries running at once, see below), `RAYON_NUM_THREADS`
  (two below: Typst layout in the tests and in the e2e server; the core builds
  two themes in threads of its own next to it), `TOKIO_WORKER_THREADS`,
  `VITEST_MAX_WORKERS` (one below), `PLAYWRIGHT_WORKERS` (a quarter of
  `CHECK_JOBS`, at least 1, at most 8: each worker has its own app server and
  browser; unset - `npm run e2e` by hand - one). Our code and its settings
  (`PARALLEL` themes) are not changed.
- The step `cargo-test` does not use `cargo test`'s one binary after another
  (`lib/cargo-test.sh`): cargo only builds (`--no-run`), then the binaries run
  at the same time, each as cargo runs it (the directory and the variables of
  its package), the longest of the last run first (their seconds are in
  `~/.cache/baluk-notes/cargo-test-times`). The longest gets half of
  `RUST_TEST_THREADS`, the others a quarter, and a binary starts when that
  many are free, so the threads running at once never exceed it (measured
  against two thirds and a third, and thirds for all); `cargo test --doc` is
  one more job. The log of each binary is
  `tests/.data/check/cargo-test/<n>.log`; the step log holds them in the order
  of the names, a table (tests, threads, seconds), and at the end the failed
  binaries with their failed tests. A plain `cargo test` (`tools/cargo.sh test
  -p <crate>`) runs the binaries in turn, each with all the threads.
- The linker (lld) takes a thread per CPU and cargo links the test binaries
  all at once: `.cargo/config.toml` limits it to one thread per link. It is a
  static entry, not `RUSTFLAGS` of the script: a flag change rebuilds
  everything, and a build by hand must share artifacts with a check.
- One lock `~/.cache/baluk-notes/heavy.lock` (`HEAVY_LOCK`): `check.sh` holds
  it for the duration of each step (the wait is not in the step time but in the
  lock wait column of `summary.md`, a line "waiting for another run" is
  printed), `install.sh` and `test-env.sh` for
  their builds, `tools/cargo.sh` for its command. Steps of different runs take
  turns, so together they stay within the cap, and one run does not rewrite
  artifacts under the tests of another. The kernel releases the lock when its
  holder dies; child processes do not inherit it. Do not take it inside a step
  (no nesting): pruning and cleaning of the shared build dir are a locked step
  of their own at the start of `check.sh`. A plain `cargo` and `npm` started
  by hand are outside the cap; `tools/cargo.sh` is within it.
- Measured on the 12-CPU laptop (CPU time of the whole run in a cgroup, windows
  of 0.5 s): a full run at `CHECK_JOBS=9` peaks at 8.9 busy cores from an empty
  target dir (before: 11.2); `CHECK_JOBS=4` on `--app` - 5.8 (the e2e browser
  and server add to the settings of the tools, so a peak can exceed
  `CHECK_JOBS` by 1-2). Not measured: two runs started at once, the cap of
  `install.sh` (same settings and lock).

**Reference snapshots** `tests/snapshots/`: the HTML of every fixture
(`*.snap`), the public library names, the search reference. A test failed -
read `git diff tests/snapshots`; the change is intended -
`UPDATE_SNAPSHOTS=1 cargo test -p notes-core`.

## Look by eye

After edits of the library, CSS or client - both themes and a narrow screen.
Screenshots go to the scratchpad or `tests/.data/`, not to the repository;
open PNG files with Read.

```sh
tools/test-env.sh [--fresh] &          # a server on tests/vault, port 8432
tools/visual.mjs [--only Книга]         # fixtures x (light, dark, narrow) -> tests/.data/visual/index.html
tools/shot.mjs "http://127.0.0.1:8432/v/vault/n/демо/компоненты" out.png [--dark] [--size 400x800] [--full]
tools/shot.mjs URL out.png --print 'document.getElementById("page").scrollTop'  # a measurement in the page (the #page column scrolls)
```

- Do not use `chromium --screenshot`: it draws the page from scratch without
  scrolling - anchors and sticky panels break in the screenshot. `shot.mjs`
  does not emulate a phone (with emulation measurements lie): a narrow screen
  is set by width.
- A debug build takes `baluk/` and `app/dist` from disk; Rust edits need a
  restart of `notes serve`.
- Stop the test server by port: `fuser -k -TERM 8432/tcp`. Not
  `pgrep -x notes-typst`: it also finds the core of the user's open window.
  Not `pkill -f ...`, and do not wait for a process via
  `pgrep -f ...`: the pattern matches the command line of the shell itself
  (it kills it or waits forever).

## No windows on the user's display

No script, check or agent run opens a window on the display of the user: it
covers the screen they work on. Whatever needs a window (WebKitGTK, the
`notes-app` window) runs inside a headless gamescope:

```sh
gamescope --backend headless -W <width> -H <height> -- <command>
```

- The output is at least as large as the window asked for.
- gamescope exits with 0 even when the command fails: check the result of the
  command itself (`shot-webkit.py` and `app-smoke.sh` read an exit code the
  command writes to a file; a command that leaves none is a failure). When
  gamescope fails or is not installed, report it and stop: never fall back to
  a visible window.
- Inside, GTK must use the X server of gamescope: gamescope gives the command
  `DISPLAY` and removes `WAYLAND_DISPLAY`, so without `GDK_BACKEND=x11` GTK
  falls back to the default socket `wayland-0` of the user's session. Set
  `GDK_BACKEND=x11` and unset `WAYLAND_DISPLAY` for the command; the scripts
  here do it, and refuse to run outside gamescope or when `DISPLAY` is the
  user's own (the outer part passes it as `BALUK_ORIG_DISPLAY`).
- Rootless Xwayland: a grab of the X root window is black, there is no
  screenshot of the whole screen. A page is captured by the engine itself
  (`shot-webkit.py`).
- Isolate a run of the app from the session of the user: its own
  `XDG_RUNTIME_DIR` (the single-instance socket and the core socket there: a
  window the user has open is not handed anything and not stopped),
  `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, a copy of the vault
  and its own `--data`: `app-smoke.sh` does it.
- Chromium (`lib/browser.mjs`: `--headless=new`; Playwright: `headless: true`
  in `app/playwright.config.ts`) has no window; do not pass `--headed`.
  `test-env.sh`, `check.sh` and `install.sh` start no window.
- Without gamescope or WebKitGTK (a cloud session) the window tools refuse
  with exit 2 and a message (`app-smoke.sh`, `shot-webkit.py`); `check.sh`,
  `install.sh` and `test-env.sh` do not need them: without WebKitGTK the
  `notes-app` window is left out of the builds (`has_webkit`).

## Cloud environment

A Claude Code cloud session clones the GitHub mirror, runs as root, has a small
disk and no Forgejo. The kit's `pipeline/scripts/cloud-setup.sh` starts a
throwaway Forgejo inside it and `pipeline/scripts/cloud-finish.sh` sends the
result to GitHub (`pipeline/docs/README.md`, "Forgejo"); the pipeline mod does
not run there (no CI runner, `docs/agents.md`, "Known limits").

**Session start** (the liaison, before anything else):
`pipeline/scripts/cloud-setup.sh`. The session may be resumed or restarted with
the server gone: run it again. At the end: `pipeline/scripts/cloud-finish.sh`,
and give the user the branch it prints.

**Environment setup script** (Bash, root, before Claude Code starts, cached in
a snapshot): only what needs root or gains from the cache. Today it installs
the wrapper `/usr/local/bin/chromium` with `--no-sandbox` (from
`/opt/pw-browsers`). The network policy allows `packages.typst.org` (CeTZ)
and, for `cloud-setup.sh`, `codeberg.org` (Forgejo) and `dl.gitea.com`
(`tea`), both without redirects. If the script sees the checkout,
`pipeline/scripts/cloud-setup.sh --install` there caches the two downloads
(~150 MB) in the snapshot; otherwise the first run downloads them.

Working in the session:

- There is no user vault - work on `tests/vault`. Fonts are embedded.
- The disk is small (full debug info is tens of GB): `tools/*.sh` set
  `CARGO_PROFILE_DEV_DEBUG=line-tables-only` themselves (`lib/build-env.sh`,
  see "Checks"). That a session counts as a cloud one rests on
  `CLAUDE_CODE_REMOTE=true`, which is an unconfirmed assumption; the explicit
  switch is `BALUK_CLOUD=1` (export it in the shell that builds if
  `CLAUDE_CODE_REMOTE` is not set). `tools/cargo.sh` sets it as the scripts do;
  a plain `cargo` needs the variable itself; out of space - `cargo clean`.
- Time: a Rust build from scratch ~8 min, `cargo test` ~1 min, e2e ~1.5 min.
- Not for the cloud (needs the user or their machine): the Tauri and Android
  apps, authorization, replacing font files, updating Typst.

## Pipeline mod

The development cycle is a kit in `pipeline/` (the mod, the agents, the skills,
`pipeline/docs/README.md`, the project layer reference `pipeline/config.md`);
this project's side is `docs/agents.md` and `.claude/pipeline.json`. The kit's
agents and skills are linked into `.claude/agents` and `.claude/skills` by
`pipeline/setup.sh`; `tools/pipeline` is a link to `pipeline/` for sessions
started with the old path.

- **Checks**: `claude plugin validate pipeline`, `claude plugin test pipeline`
  and `tsc -p pipeline` are the step `pipeline-mod` of `check.sh`. The types of
  the engine (`pipeline/.claude-plugin/types/`, not in the repository) are laid
  by Claude Code when it loads the mod from the main checkout. A linked
  worktree (agents, `check-pr-<n>`) has none: the step copies them from the
  main checkout into its own ignored folder on every run
  (the kit's `pipeline/scripts/lib/mod-check.sh`; the copy is marked
  `.copied-from-main`, a folder without the mark is laid by Claude Code and is
  left alone). `tsc` is
  `app/node_modules/.bin/tsc` of this checkout, else the main checkout's. The
  `tsc` part is left out with a line in the log only when neither checkout has
  the types or a `tsc`. Without a `claude` that has plugin tests (a cloud
  session) the step is skipped.
- **Links**: `pipeline/setup.sh [<project root>]` creates the relative links of
  the kit's agent files and skill directories under `.claude/`; it is
  idempotent and refuses, changing nothing, when a place holds a file that is
  not a link or a link to something else. `tools/test-libs.sh` tests it.
