---
paths:
  - "tools/**"
---

# tools - checks, screenshots, installation

| File | What |
|---|---|
| `check.sh` | the full check |
| `install.sh` | `notes` and its parts (`notes-typst`, the `notes-app` window with a menu entry and icon) into `~/.local/bin` and the skill into `~/.claude/skills` in one version (the autostart service is not installed; one still active is restarted); run again after changes |
| `test-env.sh` | a server on `tests/vault` (data `tests/.data`, port 8432) |
| `visual.mjs`, `shot.mjs` | screenshots of fixtures and pages (browser - `lib/browser.mjs`) |
| `shot-webkit.py` | the same page in WebKitGTK (the engine of the app window): a screenshot or a measurement, `--dark`/`--light`, `--print 'JS'`; opens a visible window for a moment |

## Checks

Tests do not touch the user's data: the fixture vault `tests/vault/` (the case
catalog - `.claude/rules/tests-vault.md`), test data - `tests/.data/` (not in
git, can be deleted).

```sh
tools/check.sh          # full, before a commit and a PR (~4 min)
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
(shared by worktrees) - no cleaning.

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
