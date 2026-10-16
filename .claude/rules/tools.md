---
paths:
  - "tools/**"
---

# tools - checks, screenshots, installation

| File | What |
|---|---|
| `check.sh` | the full check |
| `install.sh` | `notes` and its parts (`notes-typst`, `notes-hub`, the `notes-app` window with a menu entry and icon) into `~/.local/bin` and the skill into `~/.claude/skills` in one version (the autostart service is not installed; one still active is restarted); run again after changes |
| `deploy-hub.sh` | updates `notes-hub` on the VPS in one command: sends the sources of a commit over SSH, the VPS builds, a root helper (`server/baluk-notes-hub-install`, installed by hand) installs and restarts (`docs/server.md`, "Updating") |
| `test-env.sh` | a server on `tests/vault` (data `tests/.data`, port 8432, another - `PORT=...`) |
| `lib/worktree-target.sh` | in a linked worktree (agents) - the build dir shared by all worktrees |
| `lib/cpu-cap.sh` | the CPU cap `CHECK_JOBS` and the lock of heavy work, sourced by `check.sh`, `install.sh`, `test-env.sh` |
| `visual.mjs`, `shot.mjs` | screenshots of fixtures and pages (browser - `lib/browser.mjs`) |
| `shot-webkit.py` | the same page in WebKitGTK (the engine of the app window): a screenshot or a measurement, `--dark`/`--light`, `--print 'JS'`; opens a visible window for a moment |

## Checks

Tests do not touch the user's data: the fixture vault `tests/vault/` (the case
catalog - `.claude/rules/tests-vault.md`), test data - `tests/.data/` (not in
git, can be deleted).

```sh
tools/check.sh          # full, before a commit and a PR (~3 min warm)
tools/check.sh --fast   # without the client build and e2e; also --rust, --app
```

`check.sh` and `install.sh` run with lower than normal priority (`nice` 2 and
1, `ionice` 5): the computer does not lag. Steps: cargo test, clippy
(`-D warnings`), fmt; `notes check` on `tests/vault` (the total is compared
with the "Total" line in `.claude/rules/tests-vault.md`); `notes new`
templates and the skill (`.claude/rules/skills.md`); exported API types match
the committed ones (uncommitted ones also count as "failed"); the client -
check, Vitest, build, e2e. At the end - a table of steps with times, a
non-zero exit code on any failure, logs in `tests/.data/check/`. e2e needs a
system Chromium (`/usr/bin/chromium`, another one - `CHROMIUM=...`). The
`notes-app` window is built if WebKitGTK is present (`pkg-config
webkit2gtk-4.1`), otherwise it is excluded from the cargo steps (cloud).
Before the steps: `target/` over 30 GB (`TARGET_LIMIT_GB`) - `cargo clean`,
cargo does not remove old builds itself; with an own `CARGO_TARGET_DIR`
(shared by worktrees) - no cleaning. In a linked worktree `check.sh` and
`test-env.sh` build into `~/.cache/baluk-notes/worktree-target`
(`lib/worktree-target.sh`); without `app/dist` (a fresh checkout) the client
is built before the Rust steps - server tests serve it. e2e listens on fixed
ports: the lock of heavy work (below) keeps runs from overlapping.

## CPU cap

`check.sh`, `install.sh` and `test-env.sh` (its build) keep at most
`CHECK_JOBS` logical CPUs busy (default 9, never more than the machine has),
also when several runs go at once (agents in worktrees). Priority stays as
above (`nice` 2 and 1); the cap is on top of it.

- `lib/cpu-cap.sh` turns the number into the setting of each tool:
  `CARGO_BUILD_JOBS` (one below `CHECK_JOBS`; overrides `jobs` of
  `~/.cargo/config.toml`), `RUST_TEST_THREADS` (one below), `RAYON_NUM_THREADS`
  (two below: Typst layout in the tests and in the e2e server; the core builds
  two themes in threads of its own next to it), `TOKIO_WORKER_THREADS`,
  `VITEST_MAX_WORKERS` (one below). Playwright runs one worker. Our code and
  its settings (`PARALLEL` themes) are not changed.
- The linker (lld) takes a thread per CPU and cargo links the test binaries
  all at once: `.cargo/config.toml` limits it to one thread per link. It is a
  static entry, not `RUSTFLAGS` of the script: a flag change rebuilds
  everything, and a build by hand must share artifacts with a check.
- One lock `~/.cache/baluk-notes/heavy.lock` (`HEAVY_LOCK`): `check.sh` holds
  it for the duration of each step (the wait is not in the step time, a line
  "waiting for another run" is printed), `install.sh` and `test-env.sh` for
  their builds. Steps of different runs take turns, so together they stay
  within the cap, and one run does not rewrite artifacts under the tests of
  another. The kernel releases the lock when its holder dies; child processes
  do not inherit it. Do not take it inside a step (no nesting). `cargo` and
  `npm` started by hand are outside the cap.
- Measured on the 12-CPU laptop (CPU time of the whole run in a cgroup, windows
  of 0.5 s): a full run at the default peaks at 8.9 busy cores from an empty
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

## Cloud environment

- Chromium is the wrapper `/usr/local/bin/chromium` with `--no-sandbox`
  (installed by the environment setup script); fonts are embedded;
  `packages.typst.org` is allowed (CeTZ). There is no user vault - work on
  `tests/vault`.
- The disk is small: build with `CARGO_PROFILE_DEV_DEBUG=line-tables-only`
  (full debug info is tens of GB); out of space - `cargo clean`.
- Time: a Rust build from scratch ~8 min, `cargo test` ~1 min, e2e ~1.5 min.
- Not for the cloud (needs the user or their machine): the Tauri and Android
  apps, authorization, replacing font files, updating Typst.
