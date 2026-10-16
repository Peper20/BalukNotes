---
paths:
  - "tools/**"
---

# tools - checks, screenshots, installation

| File | What |
|---|---|
| `check.sh` | the full check |
| `install.sh` | `notes` and its parts (`notes-typst`, `notes-hub`, the `notes-app` window with a menu entry and icon) into `~/.local/bin` and the skill into `~/.claude/skills` in one version (the autostart service is not installed; one still active is restarted); run again after changes |
| `cloud-setup.sh`, `cloud-finish.sh` | a throwaway Forgejo in a cloud session and sending its `main` to a GitHub branch ("Cloud environment") |
| `deploy-hub.sh` | updates `notes-hub` on the VPS in one command: sends the sources of a commit over SSH, the VPS builds, a root helper (`server/baluk-notes-hub-install`, installed by hand) installs and restarts (`docs/server.md`, "Updating") |
| `test-env.sh` | a server on `tests/vault` (data `tests/.data`, port 8432, another - `PORT=...`) |
| `lib/worktree-target.sh` | in a linked worktree (agents) - the build dir of the main checkout, `<root>/target` |
| `lib/target-limit.sh` | pruning and the size limit of that dir, sourced by `check.sh` (the crate list by python3, no `jq`) |
| `lib/cpu-cap.sh` | the CPU cap `CHECK_JOBS` and the lock of heavy work and `freshen_sources`, sourced by `check.sh`, `install.sh`, `test-env.sh` |
| `lib/build-env.sh` | the build settings of a small disk and a cloud session (`CARGO_PROFILE_DEV_DEBUG`) and the WebKitGTK check, sourced by `check.sh`, `install.sh`, `test-env.sh` |
| `test-libs.sh` | tests of the libraries above without a cargo build - the first step of `check.sh` |
| `visual.mjs`, `shot.mjs` | screenshots of fixtures and pages (browser - `lib/browser.mjs`) |
| `shot-webkit.py` | the same page in WebKitGTK (the engine of the app window): a screenshot or a measurement, `--dark`/`--light`, `--print 'JS'`; runs itself inside a headless gamescope |
| `app-smoke.sh` | a smoke launch of the `notes-app` window inside a headless gamescope: it opens, loads the fixture vault, quits cleanly (~6 s); not in `check.sh` (needs gamescope and an installed or built `notes-app`) |

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
In a linked worktree `check.sh` and `test-env.sh` build into the `target/` of
the main checkout (`lib/worktree-target.sh`, an absolute `CARGO_TARGET_DIR`;
your own value is kept), so the dependencies built once serve every worktree;
only our own crates are rebuilt in a new worktree. Prefix a `cargo` by hand
with the same `CARGO_TARGET_DIR`, or it builds from scratch into the
worktree's own `target/`. A worktree is inside the main checkout, so cargo
reads `.cargo/config.toml` twice there and the `rustflags` differ from a build
in the main checkout: that build makes its own copy of the dependencies. A
`cargo` by hand in the main checkout takes no lock of ours; cargo's own
build-dir lock keeps it from overlapping a check (they wait for each other).
The unit hash of our crates does not contain the checkout path and cargo
judges them fresh by file times, so a build of another checkout, newer than
your files, would be taken for yours. The lock of heavy work (below) records
the checkout that held it last: the first holder after a run of another
checkout (`check.sh` steps, `install.sh`, `test-env.sh`) sets the times of the
tracked sources to now (`freshen_sources` in `lib/cpu-cap.sh`), and cargo
recompiles our crates against this tree. A repeated run of the same checkout
does not, so our crates are not rebuilt. Dependencies and the foreign copies
in `vendor/` stay built (after an edit under `vendor/`, `touch` the file or
`cargo clean -p` that crate). A plain `cargo` by hand is exposed to this: run
`freshen_sources` first (after `source tools/lib/cpu-cap.sh`) or use the
scripts.
Before the steps, under the lock of heavy work, `lib/target-limit.sh` keeps
the dir bounded (cargo removes nothing itself): units of our own crates that
cargo has not rebuilt for `TARGET_PRUNE_DAYS` (3) are removed
(`TARGET_PRUNE_DRY=1` - only list them), dependencies stay. The list of the
crates comes from `cargo metadata`, read by python3 (no `jq` needed); without
python3 the pruning is skipped with a `WARNING` and the size limit still
applies. Then over the limit - `cargo clean`. The limit is `TARGET_LIMIT_GB`
or, by default, half of what the dir could hold (its size plus the free space
of its disk), at most 30 GB: on a disk with little free space it is lower, and
a line `target limit:` says so. A limit that cannot be measured or cleaned is
a `WARNING` on stderr. Both skip someone else's `CARGO_TARGET_DIR`; the
pruning skips a profile while cargo holds its build lock. The vendored
`comemo` tests build into `vendor/` of the same dir.
`lib/build-env.sh` (sourced after `cpu-cap.sh`) sets
`CARGO_PROFILE_DEV_DEBUG=line-tables-only` (not in `Cargo.toml`) when the file
system of the build dir is under `SMALL_DISK_GB` (100) in total, or in a cloud
session (`CLAUDE_CODE_REMOTE=true` or `BALUK_CLOUD=1`); a value set by hand is
kept. It also answers whether WebKitGTK is installed (`has_webkit`).
`tools/test-libs.sh`, the first step of `check.sh`, tests these libraries
without a cargo build, with shims for `jq`, `df`, `python3` and `pkg-config`.
Without `app/dist` (a fresh checkout) the client is built before the Rust
steps - server tests serve it. e2e listens on fixed ports: the lock of heavy
work (below) keeps runs from overlapping.

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
  do not inherit it. Do not take it inside a step (no nesting): pruning and
  cleaning of the shared build dir are a locked step of their own at the start
  of `check.sh`. `cargo` and `npm` started by hand are outside the cap.
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

A Claude Code cloud session clones the GitHub mirror, runs as root, has a
small disk and no Forgejo. The pipeline works there as it does locally:
`tools/cloud-setup.sh` starts a throwaway Forgejo inside the session with the
same accounts, `tea` logins and protection of `main` (`docs/agents.md`,
"Server"), so the rules, skills and agent definitions stay as they are;
`tools/cloud-finish.sh` sends the result to GitHub.

**Session start** (the liaison, before anything else): `tools/cloud-setup.sh`.
It prints a summary; from then on `origin` is the cloud Forgejo and `github`
is the clone's own remote, and `git`, `tea` and the `forge` skill work as
locally. The session may be resumed or restarted with the server gone:
run the script again. At the end: `tools/cloud-finish.sh`, and give the user
the branch it prints.

**Environment setup script** (Bash, root, before Claude Code starts, cached in
a snapshot): only what needs root or gains from the cache. Today it installs
the wrapper `/usr/local/bin/chromium` with `--no-sandbox` (from
`/opt/pw-browsers`). The network policy allows `packages.typst.org` (CeTZ)
and, for `tools/cloud-setup.sh`, `codeberg.org` (Forgejo) and `dl.gitea.com`
(`tea`), both without redirects. If the script sees the checkout,
`tools/cloud-setup.sh --install` there caches the two downloads (~150 MB) in
the snapshot; otherwise the first run downloads them.

**`tools/cloud-setup.sh`** (idempotent; needs `git`, `curl`, `sha256sum`,
`awk`; no `jq`, here or in the checks; `--install` only downloads):

- Installs the pinned releases (versions and sha256 in the script; a mismatch
  stops it): Forgejo into `$FORGEJO_HOME/bin`, `tea` into `/usr/local/bin`
  (root) or `~/.local/bin` (`FORGEJO_BIN_DIR`) unless a `tea` is present.
- Initialises Forgejo on `127.0.0.1:$FORGEJO_PORT` (default 3000) with SQLite,
  registration disabled, no email, no SSH, data in `$FORGEJO_HOME` (default
  `~/.cache/baluk-notes/forgejo`; random passwords in its `credentials`, 0600).
  Forgejo refuses to run as root; the script sets its override for this
  throwaway server.
- Creates `ibaluk` (admin), `agent-dev`, `agent-reviewer`, the `tea` logins
  `owner`, `dev`, `reviewer`, the repository `ibaluk/baluk-notes` (squash
  merges only), pushes the clone's `main` into it (a shallow clone is
  completed first) and protects `main` as the local server does.
- Remotes: `origin` is the cloud Forgejo, `github` is the clone's former
  `origin`. git signs in as `ibaluk` through a helper that reads `credentials`,
  and does not use an environment proxy for the loopback address.
- Refuses, changing nothing, when the port is taken by another program, when
  `$FORGEJO_HOME` is not its own, when `origin` is already a Forgejo
  repository or `github` exists, or when the `tea` config has `owner`, `dev`
  or `reviewer` that the script did not make for this server: it never
  touches the user's own Forgejo. On the user's machine `origin` and the
  logins have exactly the URL of the default port, so a rerun is accepted
  only with the proof in `$FORGEJO_HOME` (`remotes-done`, `tea-logins`)
  that the script itself arranged them. `--stop` stops its server.
- Nothing is mirrored to GitHub, and nothing writes to GitHub `main`.

**`tools/cloud-finish.sh [--branch NAME]`**: fetches `main` of the cloud
Forgejo and pushes it to a GitHub branch: `--branch`, else `$CLOUD_BRANCH`,
else the `claude/...` branch the session started on (recorded by the setup),
else `cloud/<date>`. It refuses `main`, never forces, reports a rejected push
(the branch has commits the session lacks) with the ways out, warns about
pull requests still open on the server (they are not sent) and prints the
link for the user to open a pull request from.

Working in the session:

- There is no user vault - work on `tests/vault`. Fonts are embedded.
- The disk is small (full debug info is tens of GB): `tools/*.sh` set
  `CARGO_PROFILE_DEV_DEBUG=line-tables-only` themselves (`lib/build-env.sh`,
  see "Checks"). That a session counts as a cloud one rests on
  `CLAUDE_CODE_REMOTE=true`, which is an unconfirmed assumption; the explicit
  switch is `BALUK_CLOUD=1`, and `tools/cloud-setup.sh` prints in its summary
  which of them it sees and, if neither, the line to export. A `cargo` by hand
  needs the variable itself; out of space - `cargo clean`.
- Time: a Rust build from scratch ~8 min, `cargo test` ~1 min, e2e ~1.5 min.
- Not for the cloud (needs the user or their machine): the Tauri and Android
  apps, authorization, replacing font files, updating Typst.
